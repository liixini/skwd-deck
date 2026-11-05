use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result, ensure};
use paper_control::DrawingManifest;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::config::Config;

pub mod pack;

#[cfg(feature = "daemon")]
pub use crate::infrastructure::wallpaper::preview;

pub const RECIPE: &str = "natural-ink-v3";
pub const MANIFEST: &str = "manifest.json";
pub const PREVIEW: &str = "ink.png";
pub const PACK_RECORD: &str = "processing.json";
pub const WORK_DIRECTORY: &str = "working";
const CACHE_DIRECTORY: &str = "drawing-v1";
pub const INDEX_DIRECTORY: &str = "index";
pub const MEDIA_DIRECTORY: &str = "media";
pub const PAUSE_MARKER: &str = "paused";
const REFERENCE_DIRECTORY: &str = "references";
const ARTIFACT_DIRECTORY: &str = "prepared";
const HASH_BYTES: usize = 64;
const READ_BUFFER: usize = 64 * 1024;
const RECORD_LIMIT: u64 = 64 * 1024;
const PROCESSING_LIMIT: u64 = 32 * 1024 * 1024;

mod message {
    pub const MEDIA_CHANGED: &str = "drawing media changed";
    pub const RECIPE_CHANGED: &str = "drawing cache recipe changed";
    pub const PACK_CHANGED: &str = "drawing processing recipe changed";
    pub const REFERENCE_ESCAPE: &str = "drawing reference escapes its directory";
    pub const CACHE_ESCAPE: &str = "drawing cache escapes its directory";
    pub const BAD_IDENTITY: &str = "invalid drawing media identity";
}

#[derive(Deserialize)]
struct PackIdentity {
    fingerprint: String,
    inventory: Vec<PackStamp>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackStamp {
    pub path: PathBuf,
    link: FileStamp,
    target: FileStamp,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct FileStamp {
    device: u64,
    inode: u64,
    mode: u32,
    size: u64,
    mtime: i64,
    mtime_ns: i64,
    ctime: i64,
    ctime_ns: i64,
}

impl From<&std::fs::Metadata> for FileStamp {
    fn from(metadata: &std::fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            mode: metadata.mode(),
            size: metadata.len(),
            mtime: metadata.mtime(),
            mtime_ns: metadata.mtime_nsec(),
            ctime: metadata.ctime(),
            ctime_ns: metadata.ctime_nsec(),
        }
    }
}

impl PackStamp {
    pub fn capture(path: &Path) -> Result<Self> {
        let target = path.metadata()?;
        Ok(Self {
            path: path.to_path_buf(),
            link: FileStamp::from(&path.symlink_metadata()?),
            target: FileStamp::from(&target),
        })
    }

    pub fn valid(&self) -> bool {
        Self::capture(&self.path).is_ok_and(|current| current == *self)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CacheEntry {
    pub artifact: String,
    pub pack: String,
    pub recipe: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MediaEntry {
    pub cache: CacheEntry,
    pub signature: String,
    pub reference: PathBuf,
    #[serde(default)]
    pub source: Option<PathBuf>,
}

pub fn root(config: &Config) -> PathBuf {
    Path::new(&config.cache_dir()).join(CACHE_DIRECTORY)
}

pub fn artifacts(config: &Config) -> PathBuf {
    root(config).join(ARTIFACT_DIRECTORY)
}

pub fn references(config: &Config) -> PathBuf {
    root(config).join(REFERENCE_DIRECTORY)
}

pub fn clear(config: &Config) -> Result<()> {
    let root = root(config);
    if !root.exists() {
        return Ok(());
    }
    ensure!(
        !root.symlink_metadata()?.file_type().is_symlink(),
        "drawing cache must not be a symlink"
    );
    for name in [ARTIFACT_DIRECTORY, INDEX_DIRECTORY, MEDIA_DIRECTORY, REFERENCE_DIRECTORY] {
        let directory = root.join(name);
        if directory.exists() {
            ensure!(
                !directory.symlink_metadata()?.file_type().is_symlink(),
                "drawing cache must not be a symlink"
            );
            std::fs::remove_dir_all(directory)?;
        }
    }
    Ok(())
}

#[cfg(feature = "daemon")]
pub fn media_signature(
    state: &crate::state::WallState,
    key: &str,
    source: &Path,
) -> Result<String> {
    const CAPTURE_VARIANT: &str = "drawing-reference-2560-first-frame-v2";
    let input = if let Some(we_id) = key.strip_prefix(wall_proto::drawing::WE_KEY_PREFIX) {
        let scene = crate::we::drawing_signature(state, we_id)?;
        let fps = crate::we::scene_fps(state, we_id);
        let fill_mode =
            state.config().renderer().we_scene_fill_mode(wall_proto::apply_fields::ALL_OUTPUTS);
        serde_json::to_vec(&(CAPTURE_VARIANT, scene, fill_mode, fps))?
    } else {
        let metadata = source.metadata()?;
        serde_json::to_vec(&(CAPTURE_VARIANT, source, FileStamp::from(&metadata)))?
    };
    Ok(digest(&input))
}

fn media_path(config: &Config, source: &Path) -> Result<PathBuf> {
    let source = source.canonicalize()?;
    Ok(root(config)
        .join(MEDIA_DIRECTORY)
        .join(format!("{}.json", digest(source.as_os_str().as_encoded_bytes()))))
}

pub fn lookup_media(config: &Config, source: &Path, signature: &str) -> Result<(PathBuf, PathBuf)> {
    let entry: MediaEntry = read_record(&media_path(config, source)?)?;
    ensure!(entry.signature == signature, message::MEDIA_CHANGED);
    ensure!(
        entry.cache.recipe == RECIPE && valid_hash(&entry.cache.artifact),
        message::RECIPE_CHANGED
    );
    let active = root(config).join(PACK_RECORD);
    if active.exists() {
        ensure!(validate_processing(config)? == entry.cache.pack, message::PACK_CHANGED);
    }
    let reference = entry.reference.canonicalize()?;
    ensure!(reference.starts_with(references(config).canonicalize()?), message::REFERENCE_ESCAPE);
    let manifest = artifacts(config).join(&entry.cache.artifact).join(MANIFEST);
    ensure!(
        manifest.canonicalize()?.starts_with(artifacts(config).canonicalize()?),
        message::CACHE_ESCAPE
    );
    DrawingManifest::open(&manifest, &reference).context("validate prepared drawing")?;
    Ok((manifest, reference))
}

pub fn publish_media(config: &Config, source: &Path, entry: &MediaEntry) -> Result<()> {
    ensure!(
        valid_hash(&entry.cache.artifact) && valid_hash(&entry.signature),
        message::BAD_IDENTITY
    );
    let path = media_path(config, source)?;
    let previous = read_record::<MediaEntry>(&path).ok();
    std::fs::create_dir_all(path.parent().context("drawing media index missing")?)?;
    skwd_config::atomic_write(&path, &serde_json::to_vec(entry)?)?;
    if let Some(previous) = previous {
        if previous.reference != entry.reference
            && previous.reference.parent() == Some(references(config).as_path())
        {
            let _ = std::fs::remove_file(previous.reference);
        }
        if previous.cache.artifact != entry.cache.artifact && valid_hash(&previous.cache.artifact) {
            let _ = std::fs::remove_dir_all(artifacts(config).join(previous.cache.artifact));
        }
    }
    Ok(())
}

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn hash_file(path: &Path) -> Result<String> {
    hash_file_cancellable(path, &AtomicBool::new(false))
}

pub fn hash_file_cancellable(path: &Path, cancel: &AtomicBool) -> Result<String> {
    let mut input = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; READ_BUFFER];
    loop {
        ensure!(!cancel.load(Ordering::Acquire), "processing check cancelled");
        let count = input.read(&mut buffer)?;
        if count == 0 {
            return Ok(format!("{:x}", hash.finalize()));
        }
        hash.update(&buffer[..count]);
    }
}

pub fn read_record<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    read_record_limit(path, RECORD_LIMIT)
}

fn read_record_limit<T: serde::de::DeserializeOwned>(path: &Path, limit: u64) -> Result<T> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= limit, "drawing record is too large");
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn read_processing<T: serde::de::DeserializeOwned>(config: &Config) -> Result<T> {
    read_record_limit(&root(config).join(PACK_RECORD), PROCESSING_LIMIT)
}

pub fn validate_processing(config: &Config) -> Result<String> {
    let pack: PackIdentity = read_processing(config)?;
    ensure!(!pack.inventory.is_empty(), "processing inventory is missing");
    ensure!(pack.inventory.iter().all(PackStamp::valid), "processing files changed");
    Ok(pack.fingerprint)
}

pub fn index_path(config: &Config, source: &Path) -> Result<PathBuf> {
    let path = source.canonicalize()?;
    Ok(root(config)
        .join(INDEX_DIRECTORY)
        .join(format!("{}.json", digest(path.as_os_str().as_encoded_bytes()))))
}

pub fn lookup(config: &Config, source: &Path) -> Result<PathBuf> {
    let entry: CacheEntry = read_record(&index_path(config, source)?)?;
    ensure!(entry.recipe == RECIPE && valid_hash(&entry.artifact), message::RECIPE_CHANGED);
    let active = root(config).join(PACK_RECORD);
    if active.exists() {
        ensure!(validate_processing(config)? == entry.pack, message::PACK_CHANGED);
    }
    let artifact = artifacts(config).join(&entry.artifact).join(MANIFEST);
    let prepared = artifacts(config).canonicalize()?;
    ensure!(artifact.canonicalize()?.starts_with(&prepared), message::CACHE_ESCAPE);
    DrawingManifest::open(&artifact, source).context("validate prepared drawing")?;
    Ok(artifact)
}

pub fn publish(config: &Config, source: &Path, entry: &CacheEntry) -> Result<()> {
    ensure!(valid_hash(&entry.artifact), "invalid drawing cache identity");
    let path = index_path(config, source)?;
    let previous = read_record::<CacheEntry>(&path).ok();
    std::fs::create_dir_all(path.parent().context("drawing index directory missing")?)?;
    skwd_config::atomic_write(&path, &serde_json::to_vec(entry)?)?;
    if let Some(previous) = previous
        && previous.artifact != entry.artifact
        && valid_hash(&previous.artifact)
    {
        let _ = std::fs::remove_dir_all(artifacts(config).join(previous.artifact));
    }
    Ok(())
}

pub fn recorded(config: &Config, source: &Path, signature: Option<&str>, pack: &str) -> bool {
    let entry = match signature {
        Some(signature) => media_path(config, source)
            .and_then(|path| read_record::<MediaEntry>(&path))
            .ok()
            .filter(|entry| entry.signature == signature && entry.reference.is_file())
            .map(|entry| entry.cache),
        None => index_path(config, source).and_then(|path| read_record(&path)).ok(),
    };
    let Some(entry) = entry.filter(|entry: &CacheEntry| {
        entry.recipe == RECIPE && entry.pack == pack && valid_hash(&entry.artifact)
    }) else {
        return false;
    };
    let manifest = artifacts(config).join(&entry.artifact).join(MANIFEST);
    if signature.is_some() {
        return manifest.is_file();
    }
    let (Ok(manifest), Ok(metadata)) =
        (read_record::<paper_control::DrawingManifest>(&manifest), source.metadata())
    else {
        return false;
    };
    manifest.source.size == metadata.len() && stamp(&metadata) == Some(manifest.source.mtime_ns)
}

fn stamp(metadata: &std::fs::Metadata) -> Option<u128> {
    let age = metadata.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(age.as_nanos())
}

pub fn prune(config: &Config) -> Result<()> {
    let roots: Vec<(PathBuf, bool)> =
        [PathBuf::from(config.wallpaper_dir()), PathBuf::from(config.video_dir()), config.we_dir()]
            .into_iter()
            .filter_map(|root| root.canonicalize().ok())
            .map(|root| {
                let mounted = root.read_dir().is_ok_and(|mut entries| entries.next().is_some());
                (root, mounted)
            })
            .collect();
    let removed = |source: &Path| {
        !source.exists()
            && source.parent().is_some_and(Path::is_dir)
            && roots
                .iter()
                .filter(|(root, _)| source.starts_with(root))
                .max_by_key(|(root, _)| root.components().count())
                .is_some_and(|(_, mounted)| *mounted)
    };
    let pack = validate_processing(config).ok();
    let mut artifacts_kept = std::collections::BTreeSet::new();
    let mut references_kept = std::collections::BTreeSet::new();
    for path in records(&root(config).join(INDEX_DIRECTORY)) {
        let live = read_record::<CacheEntry>(&path).ok().filter(|entry| {
            entry.recipe == RECIPE
                && pack.as_deref().is_none_or(|pack| pack == entry.pack)
                && read_record::<paper_control::DrawingManifest>(
                    &artifacts(config).join(&entry.artifact).join(MANIFEST),
                )
                .is_ok_and(|manifest| !removed(&manifest.source.path))
        });
        match live {
            Some(entry) => {
                artifacts_kept.insert(entry.artifact);
            }
            None => std::fs::remove_file(path)?,
        }
    }
    for path in records(&root(config).join(MEDIA_DIRECTORY)) {
        let live = read_record::<MediaEntry>(&path).ok().filter(|entry| {
            entry.cache.recipe == RECIPE
                && pack.as_deref().is_none_or(|pack| pack == entry.cache.pack)
                && entry.reference.is_file()
                && !entry.source.as_deref().is_some_and(&removed)
        });
        match live {
            Some(entry) => {
                artifacts_kept.insert(entry.cache.artifact);
                references_kept.insert(entry.reference);
            }
            None => std::fs::remove_file(path)?,
        }
    }
    for path in records(&artifacts(config)) {
        let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        if valid_hash(name) && !artifacts_kept.contains(name) {
            std::fs::remove_dir_all(path)?;
        }
    }
    for path in records(&references(config)) {
        if path.is_file() && !references_kept.contains(&path) {
            std::fs::remove_file(path)?;
        }
    }
    for path in records(&root(config).join(WORK_DIRECTORY)) {
        std::fs::remove_dir_all(path)?;
    }
    Ok(())
}

fn records(directory: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(directory).into_iter().flatten().flatten().map(|entry| entry.path()).collect()
}

pub fn valid_hash(hash: &str) -> bool {
    hash.len() == HASH_BYTES && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn directory_bytes(path: &Path) -> Result<u64> {
    let mut bytes = 0_u64;
    if !path.exists() {
        return Ok(bytes);
    }
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_file() {
            bytes = bytes.saturating_add(entry.metadata()?.len());
        } else if kind.is_dir() {
            bytes = bytes.saturating_add(directory_bytes(&entry.path())?);
        }
    }
    Ok(bytes)
}

#[cfg(test)]
#[path = "drawing/tests.rs"]
mod tests;
