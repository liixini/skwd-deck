use std::collections::BTreeMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::state::WallState;

static REQUESTS: Mutex<BTreeMap<String, u64>> = Mutex::new(BTreeMap::new());
const THUMBNAIL_SETTLE: Duration = Duration::from_millis(1200);

pub struct ThumbnailCapture {
    _guard: std::fs::File,
    we_id: String,
    source: PathBuf,
    properties: serde_json::Map<String, serde_json::Value>,
    input: serde_json::Value,
    artifacts: [PathBuf; 4],
}

impl ThumbnailCapture {
    pub fn try_open(state: &WallState, we_id: &str) -> anyhow::Result<Option<Self>> {
        anyhow::ensure!(super::valid_we_id(we_id), "invalid Wallpaper Engine id");
        let directory = crate::paths::cache_dir().join("scene-thumbnails");
        std::fs::create_dir_all(&directory)?;
        let guard = std::fs::File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(directory.join(format!("{we_id}.lock")))?;
        match guard.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => return Ok(None),
            Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
        }
        let source = state.config().we_dir().join(we_id);
        let properties = super::scene_overrides(state, we_id);
        let input = signature(&source, &properties)?;
        let thumb = crate::paths::we_thumb(we_id);
        let (near, far) = crate::paths::thumbnail_blocks(&thumb.to_string_lossy());
        Ok(Some(Self {
            _guard: guard,
            we_id: we_id.into(),
            source,
            properties,
            input,
            artifacts: [thumb, crate::paths::we_thumb_sm(we_id), near, far],
        }))
    }

    pub fn source(&self) -> &Path {
        &self.source
    }

    pub fn properties(&self) -> &serde_json::Map<String, serde_json::Value> {
        &self.properties
    }

    pub fn cached(&self, state: &WallState) -> anyhow::Result<bool> {
        let record = state
            .with_db(|conn| crate::db::thumbnail_cache(conn, &format!("we:{}", self.we_id)))?;
        Ok(cached(record.as_ref(), &self.input, &self.artifacts))
    }

    pub fn unchanged(&self, state: &WallState) -> anyhow::Result<bool> {
        let properties = super::scene_overrides(state, &self.we_id);
        Ok(signature(&self.source, &properties)? == self.input)
    }

    pub fn record(&self, state: &WallState) -> anyhow::Result<()> {
        anyhow::ensure!(self.unchanged(state)?, "scene changed during thumbnail capture");
        let record = serde_json::json!({"input": self.input, "artifacts": artifact_stamps(&self.artifacts)?});
        let updated = state.with_db(|conn| {
            crate::db::update_thumbnail_paths(
                conn,
                &format!("we:{}", self.we_id),
                &self.artifacts[0].to_string_lossy(),
                &self.artifacts[1].to_string_lossy(),
                Some(&record),
                true,
            )
        })?;
        anyhow::ensure!(updated == 1, "scene was removed during thumbnail capture");
        Ok(())
    }
}

pub(super) fn schedule(
    state: &WallState,
    key: &str,
    we_id: &str,
    properties: &serde_json::Map<String, serde_json::Value>,
) {
    if !super::valid_we_id(we_id) {
        return;
    }
    let Some(pid) = state.renderers().video_paper_pid(key) else { return };
    let renderers = state.renderers_shared();
    let database = state.database();
    let source = state.config().we_dir().join(we_id);
    let directory = crate::paths::cache_dir().join("scene-thumbnails");
    let scanner = crate::paths::sibling_bin("skwd-wall-scan");
    let properties = properties.clone();
    let generation = {
        let mut requests = crate::lock(&REQUESTS);
        let generation = requests.entry(key.to_owned()).or_default();
        *generation += 1;
        *generation
    };
    let key = key.to_string();
    let we_id = we_id.to_string();
    std::thread::spawn(move || {
        let result = (|| -> anyhow::Result<()> {
            let current = || {
                renderers.video_paper_pid(&key) == Some(pid)
                    && crate::lock(&REQUESTS).get(&key) == Some(&generation)
            };
            std::fs::create_dir_all(&directory)?;
            let guard = std::fs::File::options()
                .create(true)
                .truncate(false)
                .write(true)
                .open(directory.join(format!("{we_id}.lock")))?;
            guard.lock()?;
            let item_key = format!("we:{we_id}");
            if !database.with_connection(|conn| Ok(crate::db::has_entry(conn, &item_key)))? {
                return Ok(());
            }
            std::thread::sleep(THUMBNAIL_SETTLE);
            if !current() {
                return Ok(());
            }
            let input = signature(&source, &properties)?;
            let thumb = crate::paths::we_thumb(&we_id);
            let (near, far) = crate::paths::thumbnail_blocks(&thumb.to_string_lossy());
            let artifacts = [thumb, crate::paths::we_thumb_sm(&we_id), near, far];
            let record =
                database.with_connection(|conn| crate::db::thumbnail_cache(conn, &item_key))?;
            if !current() || cached(record.as_ref(), &input, &artifacts) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_secs(2));
            if !current() {
                return Ok(());
            }
            let temporary = tempfile::tempdir_in(&directory)?;
            let frame = temporary.path().join("frame.png");
            if !renderers.capture_scene(
                &key,
                pid,
                &source.to_string_lossy(),
                &frame.to_string_lossy(),
            ) {
                return Ok(());
            }
            wait_frame(&frame)?;
            if !current() || signature(&source, &properties)? != input {
                return Ok(());
            }
            let status = Command::new(scanner)
                .arg("--scene-thumbnail")
                .arg(&we_id)
                .arg(&frame)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .status()?;
            anyhow::ensure!(status.success(), "thumbnail helper failed: {status}");
            if current() && signature(&source, &properties)? == input {
                let record = serde_json::json!({
                    "input": input,
                    "artifacts": artifact_stamps(&artifacts)?,
                });
                database.with_connection(|conn| {
                    crate::db::update_thumbnail_paths(
                        conn,
                        &item_key,
                        &artifacts[0].to_string_lossy(),
                        &artifacts[1].to_string_lossy(),
                        Some(&record),
                        true,
                    )
                })?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            log::warn!("we thumbnail {we_id}: {error}");
        }
    });
}

pub fn reset_thumbnail(state: &WallState, we_id: &str) -> anyhow::Result<bool> {
    anyhow::ensure!(super::valid_we_id(we_id), "invalid Wallpaper Engine id");
    let directory = crate::paths::cache_dir().join("scene-thumbnails");
    std::fs::create_dir_all(&directory)?;
    let guard = std::fs::File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(directory.join(format!("{we_id}.lock")))?;
    guard.lock()?;
    anyhow::ensure!(
        state.database().with_connection(|conn| {
            crate::db::invalidate_thumbnail_cache(conn, &format!("we:{we_id}"))
        })?,
        "Wallpaper Engine scene is not in the library"
    );
    let source = state.config().we_dir().join(we_id);
    let renderer = state.renderers().assignments().into_iter().find_map(|(output, path)| {
        (Path::new(&path) == source)
            .then(|| state.renderers().scene_paper_key_for(&output))
            .flatten()
            .filter(|key| state.renderers().video_paper_pid(key).is_some())
    });
    drop(guard);
    if let Some(key) = renderer {
        schedule(state, &key, we_id, &super::scene_overrides(state, we_id));
        return Ok(true);
    }
    Ok(false)
}

fn file_stamp(path: &Path) -> std::io::Result<serde_json::Value> {
    let meta = path.metadata()?;
    if !meta.is_file() || meta.len() == 0 {
        return Err(std::io::Error::other("thumbnail cache input is not a nonempty file"));
    }
    Ok(serde_json::json!([
        path,
        meta.len(),
        meta.mtime(),
        meta.mtime_nsec(),
        meta.ctime(),
        meta.ctime_nsec()
    ]))
}

fn signature(
    source: &Path,
    properties: &serde_json::Map<String, serde_json::Value>,
) -> std::io::Result<serde_json::Value> {
    let mut pending = paper_control::we_project::Project::resolve(source)?.directories;
    let mut files = BTreeMap::new();
    let mut visited = std::collections::BTreeSet::new();
    while let Some(directory) = pending.pop() {
        if !visited.insert(directory.clone()) {
            continue;
        }
        for entry in std::fs::read_dir(directory)? {
            let path = entry?.path().canonicalize()?;
            if path.is_dir() {
                pending.push(path);
            } else {
                let meta = path.metadata()?;
                files.insert(
                    path,
                    (meta.len(), meta.mtime(), meta.mtime_nsec(), meta.ctime(), meta.ctime_nsec()),
                );
            }
        }
    }
    Ok(
        serde_json::json!({"version": 1, "source": source, "files": files, "properties": properties}),
    )
}

fn artifact_stamps(artifacts: &[PathBuf]) -> std::io::Result<Vec<serde_json::Value>> {
    artifacts.iter().map(|path| file_stamp(path)).collect()
}

fn cached(
    record: Option<&serde_json::Value>,
    input: &serde_json::Value,
    artifacts: &[PathBuf],
) -> bool {
    let Some(record) = record else { return false };
    record.get("input") == Some(input)
        && artifact_stamps(artifacts)
            .is_ok_and(|stamps| record.get("artifacts") == Some(&serde_json::json!(stamps)))
}

fn wait_frame(frame: &Path) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let error_path = frame.with_extension("png.error");
    loop {
        if frame.is_file() {
            return Ok(());
        }
        if let Ok(error) = std::fs::read_to_string(&error_path) {
            anyhow::bail!("{error}");
        }
        anyhow::ensure!(Instant::now() < deadline, "scene capture timed out");
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[cfg(test)]
#[path = "thumbnail_tests.rs"]
mod tests;
