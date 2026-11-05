#![cfg(test)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

use tokio::sync::mpsc::Receiver;

use skwd_wall_core::WallState;
use wall_proto::{Request, Response};

use crate::backend::workers::MediaWorkerSupervisor;
use crate::infrastructure::events::EventHub;

const BIN: &str = "bin";
const DRAWN_TEXTURE: &str = "ink.png";
const DRAWN_STROKES: &str = "strokes.bin";
const EXECUTABLE: u32 = 0o700;

static ROOT: LazyLock<PathBuf> = LazyLock::new(|| {
    let root = std::env::temp_dir().join(format!("skwd-walld-tests-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for sub in ["config/skwd-wall-v2", "data", "cache", "run", "walls", "videos", BIN] {
        std::fs::create_dir_all(root.join(sub)).unwrap();
    }
    unsafe {
        std::env::set_var("HOME", &root);
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
        std::env::set_var("XDG_CACHE_HOME", root.join("cache"));
        std::env::set_var("SKWD_WALL_V2_SOCK", root.join("run/wall.sock"));
        std::env::set_var("SKWD_PAPER_V2_SOCKET", root.join("run/paper.sock"));
        std::env::set_var("SKWD_DRAWING_HOME", root.join("drawing-system"));
        std::env::set_var(
            "PATH",
            std::env::join_paths(
                std::iter::once(root.join(BIN))
                    .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())),
            )
            .unwrap(),
        );
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("WAYLAND_SOCKET");
        std::env::remove_var("SKWD_WALL_CONFIG");
        std::env::remove_var("SKWD_WALL_V2_CACHE");
        std::env::remove_var("SKWD_WALL_PAPER_STILL");
        std::env::remove_var("SKWD_WALL_DEBUG");
    }
    root
});

static LOCK: Mutex<()> = Mutex::new(());
static IMAGE_OPTIMIZE_CALLS: AtomicUsize = AtomicUsize::new(0);
static SCAN_CALLS: AtomicUsize = AtomicUsize::new(0);

pub(crate) fn scan_calls() -> usize {
    SCAN_CALLS.load(Ordering::Acquire)
}
pub(crate) fn lock() -> (MutexGuard<'static, ()>, PathBuf) {
    let guard = skwd_wall_core::lock(&LOCK);
    (guard, ROOT.clone())
}

pub(crate) fn reset_image_optimize_calls() {
    IMAGE_OPTIMIZE_CALLS.store(0, Ordering::Release);
}

pub(crate) fn image_optimize_calls() -> usize {
    IMAGE_OPTIMIZE_CALLS.load(Ordering::Acquire)
}

fn merge(base: &mut serde_json::Value, extra: serde_json::Value) {
    match (base, extra) {
        (serde_json::Value::Object(base), serde_json::Value::Object(extra)) => {
            for (key, value) in extra {
                match base.get_mut(&key) {
                    Some(slot) => merge(slot, value),
                    None => {
                        base.insert(key, value);
                    }
                }
            }
        }
        (slot, value) => *slot = value,
    }
}

pub(crate) fn write_config(extra: serde_json::Value) {
    let root = &*ROOT;
    let mut cfg = serde_json::json!({
        "paths": {
            "wallpaper": root.join("walls").to_string_lossy(),
            "videoWallpaper": root.join("videos").to_string_lossy(),
            "paperStillBin": "/nonexistent/skwd-test-paper-still",
            "paperVkBin": "/nonexistent/skwd-test-paper-vk",
        },
        "niri": { "backdropFollowWallpaper": false },
        "general": { "notifyOnWallpaperChange": false },
    });
    merge(&mut cfg, extra);
    std::fs::write(root.join("config/skwd-wall-v2/config.json"), cfg.to_string()).unwrap();
}

struct TestWorkers;

impl MediaWorkerSupervisor for TestWorkers {
    fn capture_scene_thumbnails(&self) -> bool {
        true
    }
    fn stop_scene_thumbnails(&self) -> bool {
        true
    }
    fn scan(&self, _extra: &[&str], _request_id: Option<&str>) {
        SCAN_CALLS.fetch_add(1, Ordering::AcqRel);
    }

    fn remote_thumbnails(&self, _source: &str, _jobs: &[(String, String)]) {}

    fn optimize_images(&self, _automatic: bool, _changed_paths: &[String]) -> bool {
        IMAGE_OPTIMIZE_CALLS.fetch_add(1, Ordering::AcqRel);
        true
    }

    fn image_optimization_status(&self) -> serde_json::Value {
        serde_json::json!({ "running": false })
    }
}

fn test_workers() -> Arc<dyn MediaWorkerSupervisor> {
    Arc::new(TestWorkers)
}

pub(crate) fn harness() -> (Arc<WallState>, Arc<EventHub>, Arc<crate::infrastructure::stats::Stats>)
{
    let state = Arc::new(WallState::open().unwrap());
    state.reconcile_library().unwrap();
    let stats = Arc::new(crate::infrastructure::stats::Stats::new());
    let events = Arc::new(EventHub::new(Arc::clone(&stats)));
    (state, events, stats)
}

pub(crate) fn context(
    state: &Arc<WallState>,
    events: &Arc<EventHub>,
    stats: &Arc<crate::infrastructure::stats::Stats>,
) -> crate::composition::context::Ctx {
    crate::composition::context::Ctx {
        drawing: Arc::default(),
        apply_queue: Arc::default(),
        state: Arc::clone(state),
        config: state.config_store(),
        database: state.database(),
        renderers: state.renderer_supervisor(),
        wallpaper: Arc::new(
            skwd_wall_core::infrastructure::wallpaper::CoreWallpaperApplication::new(Arc::clone(
                state,
            )),
        ),
        history: Arc::new(crate::infrastructure::history::FileHistoryRepository::new(
            state.config().cache_dir(),
        )),
        events: Arc::clone(events),
        workers: test_workers(),
        stats: Arc::clone(stats),
        tasks: Arc::new(crate::infrastructure::tasks::TaskRegistry::new(Arc::clone(events))),
    }
}

pub(crate) fn call(
    state: &Arc<WallState>,
    events: &Arc<EventHub>,
    stats: &Arc<crate::infrastructure::stats::Stats>,
    method: &str,
    params: serde_json::Value,
) -> Response {
    let req = Request { method: method.into(), params, id: 7 };
    let ctx = context(state, events, stats);
    crate::infrastructure::rpc::dispatch(&ctx, &req)
}

pub(crate) fn subscribe(events: &EventHub) -> Receiver<String> {
    let (tx, rx) = tokio::sync::mpsc::channel(64);
    events.subscribe(999, tx);
    rx
}

pub(crate) fn events(rx: &mut Receiver<String>) -> Vec<wall_proto::Event> {
    let mut collected = Vec::new();
    while let Ok(message) = rx.try_recv() {
        collected.push(serde_json::from_str(&message).unwrap());
    }
    collected
}

pub(crate) fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap()
}

pub(crate) fn ecode(response: &Response) -> i32 {
    response.error.as_ref().expect("error response").code
}

pub(crate) fn emsg(response: &Response) -> String {
    response.error.as_ref().expect("error response").message.clone()
}

pub(crate) fn rr(response: Response) -> serde_json::Value {
    assert!(response.error.is_none(), "{:?}", response.error);
    response.result.expect("result")
}

pub(crate) struct FakeBin(PathBuf);

impl FakeBin {
    pub(crate) fn install(root: &std::path::Path, name: &str, script: &str) -> Self {
        use std::os::unix::fs::PermissionsExt;

        let path = root.join(BIN).join(name);
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(EXECUTABLE)).unwrap();
        Self(path)
    }
}

impl Drop for FakeBin {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

const SCANNER: &str = r##"#!/bin/sh
case "$1" in
--semantic) printf '%s' '{"bg":"#101418","surface":"#1c2026","fg":"#e0e2e8","dim":"#8c9199","accent":"#4f8cc9","dark":true}' ;;
--theme-preview) printf '%s' '["#4f8cc9"]' ;;
*) exit 1 ;;
esac
"##;

pub(crate) fn scanner(root: &std::path::Path) -> FakeBin {
    FakeBin::install(root, skwd_wall_core::paths::SCAN_BIN, SCANNER)
}

pub(crate) fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "skwd-test-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub(crate) fn cache_item(state: &WallState, name: &str) {
    state
        .with_db(|conn| {
            skwd_wall_core::db::upsert_cache_entry(
                conn,
                &format!("{}:{name}", wall_proto::kind::STATIC),
                wall_proto::kind::STATIC,
                name,
                "",
                "",
                "",
                "",
                1,
                0,
                0,
                0,
                1,
                1,
                1,
            )
        })
        .unwrap();
}

pub(crate) fn drawn(config: &skwd_wall_core::config::Config, source: &std::path::Path) -> PathBuf {
    use skwd_wall_core::infrastructure::drawing as cache;
    let entry = cache::CacheEntry {
        artifact: cache::digest(source.as_os_str().as_encoded_bytes()),
        pack: cache::digest(b"pack"),
        recipe: cache::RECIPE.into(),
    };
    let directory = cache::artifacts(config).join(&entry.artifact);
    std::fs::create_dir_all(&directory).unwrap();
    let texture = PathBuf::from(DRAWN_TEXTURE);
    let strokes = PathBuf::from(DRAWN_STROKES);
    std::fs::write(directory.join(&texture), b"ink").unwrap();
    std::fs::write(directory.join(&strokes), []).unwrap();
    let metadata = source.metadata().unwrap();
    let manifest = paper_control::DrawingManifest {
        version: paper_control::DRAWING_VERSION,
        source: paper_control::DrawingSource {
            path: source.to_path_buf(),
            size: metadata.len(),
            mtime_ns: metadata
                .modified()
                .unwrap()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            sha256: cache::hash_file(source).unwrap(),
        },
        width: 10,
        height: 10,
        texture,
        strokes,
        timing_width: 8,
        timing_height: 8,
        focus: [0.5, 0.5],
    };
    std::fs::write(directory.join(cache::MANIFEST), serde_json::to_vec(&manifest).unwrap())
        .unwrap();
    cache::publish(config, source, &entry).unwrap();
    directory
}
