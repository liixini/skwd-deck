#![cfg(test)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

use tokio::sync::mpsc::Receiver;

use skwd_wall_core::WallState;
use wall_proto::{Request, Response};

use crate::backend::workers::{MediaWorkerSupervisor, TinierPreparation};
use crate::infrastructure::events::EventHub;

static ROOT: LazyLock<PathBuf> = LazyLock::new(|| {
    let root = std::env::temp_dir().join(format!("skwd-walld-tests-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for sub in ["config/skwd-wall-v2", "data", "cache", "run", "walls", "videos"] {
        std::fs::create_dir_all(root.join(sub)).unwrap();
    }
    unsafe {
        std::env::set_var("HOME", &root);
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
        std::env::set_var("XDG_CACHE_HOME", root.join("cache"));
        std::env::set_var("XDG_RUNTIME_DIR", root.join("run"));
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
#[derive(Clone, Copy)]
pub(crate) enum TinierTestOutcome {
    Success,
    Failure,
    Cancelled,
    DelayedSuccess,
}
static TINIER_OUTCOME: Mutex<TinierTestOutcome> = Mutex::new(TinierTestOutcome::Failure);

pub(crate) fn set_tinier_outcome(outcome: TinierTestOutcome) {
    *skwd_wall_core::lock(&TINIER_OUTCOME) = outcome;
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
    fn scan(&self, _extra: &[&str], _request_id: Option<&str>) {}

    fn remote_thumbnails(&self, _source: &str, _jobs: &[(String, String)]) {}

    fn optimize_images(&self, _automatic: bool, _changed_paths: &[String]) -> bool {
        IMAGE_OPTIMIZE_CALLS.fetch_add(1, Ordering::AcqRel);
        true
    }

    fn image_optimization_status(&self) -> serde_json::Value {
        serde_json::json!({ "running": false })
    }

    fn prepare_tinier(&self, _source: &str) -> TinierPreparation {
        let (sender, result) = std::sync::mpsc::channel();
        match *skwd_wall_core::lock(&TINIER_OUTCOME) {
            TinierTestOutcome::Success => {
                let _ = sender.send(Ok(()));
            }
            TinierTestOutcome::Failure => {
                let _ =
                    sender.send(Err(String::from("Tinier preparation unavailable in test worker")));
            }
            TinierTestOutcome::Cancelled => {
                let _ = sender.send(Err(String::from("Preparation cancelled")));
            }
            TinierTestOutcome::DelayedSuccess => {
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(30));
                    let _ = sender.send(Ok(()));
                });
            }
        }
        TinierPreparation { task_id: String::from("tinier:test"), result }
    }

    fn stop_tinier(&self, _task_id: &str) -> bool {
        false
    }
}

fn test_workers() -> Arc<dyn MediaWorkerSupervisor> {
    Arc::new(TestWorkers)
}

pub(crate) fn harness() -> (Arc<WallState>, Arc<EventHub>, Arc<crate::infrastructure::stats::Stats>)
{
    let state = Arc::new(WallState::open().unwrap());
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
