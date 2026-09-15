use std::sync::Arc;

use serde_json::json;
use skwd_wall_core::{WallState, db};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use wall_proto::ev;

use crate::backend::events::EventPublisher;
use crate::backend::workers::MediaWorkerSupervisor;
use crate::infrastructure::events::EventHub;
use crate::infrastructure::stats::Stats;

const WATCH_DEBOUNCE: std::time::Duration = std::time::Duration::from_secs(2);
const WATCH_MAX_HOLD: std::time::Duration = std::time::Duration::from_secs(10);
const WATCH_BATCH_CAP: usize = 512;
static WATCH_SCAN_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

mod polling;
mod session;
pub(crate) use session::start_watcher;
mod status;

fn watch_status_path() -> std::path::PathBuf {
    skwd_wall_core::paths::cache_dir().join("watch-status.json")
}

fn write_status(path: &std::path::Path, status: &serde_json::Value) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ =
        skwd_wall_core::paths::atomic_write_mode(path, status.to_string().as_bytes(), Some(0o600));
}

fn record_watch_failure(publisher: &dyn EventPublisher, detail: &str) {
    log::warn!("live import watch unavailable: {detail}");
    status::record_unavailable(publisher, detail);
    publisher.publish(ev::WATCH_ERROR, json!({ "detail": detail, "mode": "unavailable" }));
}

fn record_polling_fallback(
    publisher: &dyn EventPublisher,
    roots: &[polling::PollingRoot],
    interval: std::time::Duration,
) {
    let paths = roots.iter().map(|root| root.path.to_string_lossy()).collect::<Vec<_>>();
    log::warn!(
        "native library watch unavailable for {}; polling every {}s with {} entries per root per interval",
        paths.join(", "),
        interval.as_secs(),
        polling::ENTRY_BUDGET_PER_ROOT,
    );
    status::record_polling(publisher, roots, interval);
    publisher.publish(
        ev::WATCH_ERROR,
        json!({
            "detail": "native library watch failed; bounded polling fallback is active",
            "mode": "polling",
            "interval_seconds": interval.as_secs(),
            "roots": paths,
        }),
    );
}

pub(crate) fn current_status() -> wall_proto::LibraryWatchStatus {
    status::snapshot()
}

pub(crate) fn complete_scan(
    publisher: &dyn EventPublisher,
    request_id: &str,
) -> Option<wall_proto::LibraryWatchStatus> {
    status::complete_scan(publisher, request_id)
}

const TRANSIENT_EXTS: [&str; 5] = ["tmp", "temp", "ytdl", "crdownload", "download"];

fn is_transient(path: &std::path::Path) -> bool {
    if let Some(ext) =
        path.extension().and_then(std::ffi::OsStr::to_str).map(str::to_ascii_lowercase)
        && (ext.starts_with("part") || TRANSIENT_EXTS.contains(&ext.as_str()))
    {
        return true;
    }
    path.file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .and_then(|stem| stem.rsplit_once(".f"))
        .is_some_and(|(head, id)| {
            !head.is_empty()
                && id.starts_with(|ch: char| ch.is_ascii_digit())
                && id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        })
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ConfigWatch {
    path: std::path::PathBuf,
    target: std::path::PathBuf,
}

impl ConfigWatch {
    pub(crate) fn new(path: std::path::PathBuf) -> Self {
        let target = Self::resolve(&path);
        Self { path, target }
    }

    fn resolve(path: &std::path::Path) -> std::path::PathBuf {
        skwd_config::follow_links(path).unwrap_or_else(|_| path.to_path_buf())
    }

    pub(crate) fn path(&self) -> &std::path::Path {
        &self.path
    }

    fn hit(&self, path: &std::path::Path) -> bool {
        path == self.path || path == self.target
    }

    fn sibling(&self, path: &std::path::Path) -> bool {
        let parent = path.parent();
        parent == self.path.parent() || parent == self.target.parent()
    }

    fn target_dir(&self) -> Option<&std::path::Path> {
        self.target.parent().filter(|dir| Some(*dir) != self.path.parent())
    }

    fn dirs(&self) -> impl Iterator<Item = &std::path::Path> {
        self.path.parent().into_iter().chain(self.target_dir())
    }

    fn refresh(&mut self, watcher: Option<&mut notify::RecommendedWatcher>) -> bool {
        use notify::Watcher;

        let target = Self::resolve(&self.path);
        if target == self.target {
            return false;
        }
        let stale = self.target_dir().map(std::path::Path::to_path_buf);
        self.target = target;
        if let Some(watcher) = watcher {
            if let Some(dir) = stale.filter(|dir| Some(dir.as_path()) != self.target_dir()) {
                let _ = watcher.unwatch(&dir);
            }
            watch_config_dir(watcher, self);
        }
        true
    }
}

fn split_config_events(
    paths: Vec<std::path::PathBuf>,
    cfg: &ConfigWatch,
    out: &mut Vec<std::path::PathBuf>,
) -> bool {
    let mut hit = false;
    for path in paths {
        if skwd_wall_core::paths::is_internal_library_path(&path) {
            continue;
        }
        if cfg.hit(&path) {
            hit = true;
        } else if !cfg.sibling(&path) && !is_transient(&path) {
            push_capped(out, path, WATCH_BATCH_CAP);
        }
    }
    hit
}

fn push_capped(vec: &mut Vec<std::path::PathBuf>, path: std::path::PathBuf, cap: usize) -> bool {
    if vec.len() >= cap || vec.contains(&path) {
        return false;
    }
    vec.push(path);
    true
}

fn hold_exceeded(elapsed: std::time::Duration, max: std::time::Duration) -> bool {
    elapsed >= max
}

fn plan_watch_flush(
    pending: Vec<std::path::PathBuf>,
    removed: Vec<std::path::PathBuf>,
) -> (Vec<std::path::PathBuf>, Vec<std::path::PathBuf>) {
    let (changed, gone): (Vec<_>, Vec<_>) = pending.into_iter().partition(|path| path.exists());
    let mut to_remove = removed;
    to_remove.extend(gone);
    to_remove.sort();
    to_remove.dedup();
    to_remove.retain(|path| !path.exists());
    (changed, to_remove)
}

fn flush_watch_batch(
    state: &Arc<WallState>,
    publisher: &dyn EventPublisher,
    workers: &dyn MediaWorkerSupervisor,
    stats: &Stats,
    pending: Vec<std::path::PathBuf>,
    removed: Vec<std::path::PathBuf>,
    request_id: Option<&str>,
    force_full_scan: bool,
) -> bool {
    let (changed, to_remove) = plan_watch_flush(pending, removed);
    let mut removals_ok = true;
    for path in &to_remove {
        removals_ok &= handle_remove(state, publisher, path);
    }
    if changed.is_empty() && !force_full_scan {
        if !removals_ok {
            stats.set_task("scanning");
            workers.scan(&[], request_id);
            return true;
        }
        return false;
    }
    stats.set_task("scanning");
    if force_full_scan || changed.len() >= WATCH_BATCH_CAP || !removals_ok {
        if removals_ok {
            log::info!("mass change settled ({} paths), full rescan", changed.len());
        } else {
            log::warn!("delta removal did not reach the database; requesting a full rescan");
        }
        workers.scan(&[], request_id);
    } else {
        log::info!("file change settled, delta scan of {} paths", changed.len());
        let mut extra: Vec<String> = Vec::with_capacity(changed.len() + 1);
        extra.push("--paths".to_string());
        extra.extend(changed.iter().map(|path| path.to_string_lossy().into_owned()));
        let refs: Vec<&str> = extra.iter().map(String::as_str).collect();
        workers.scan(&refs, request_id);
    }
    true
}

fn next_scan_request_id() -> String {
    let sequence = WATCH_SCAN_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("watch-{}-{sequence}", std::process::id())
}

fn create_watcher(
    tx: UnboundedSender<notify::Event>,
) -> Result<notify::RecommendedWatcher, String> {
    notify::recommended_watcher(move |result: notify::Result<notify::Event>| match result {
        Ok(event) => {
            let _ = tx.send(event);
        }
        Err(error) => log::warn!("native library watcher reported an error: {error}"),
    })
    .map_err(|error| error.to_string())
}

trait RootWatcher {
    fn watch_root(&mut self, path: &std::path::Path) -> Result<(), String>;
}

impl RootWatcher for notify::RecommendedWatcher {
    fn watch_root(&mut self, path: &std::path::Path) -> Result<(), String> {
        use notify::Watcher;

        self.watch(path, notify::RecursiveMode::Recursive).map_err(|error| error.to_string())
    }
}

fn media_roots(state: &Arc<WallState>) -> Vec<std::path::PathBuf> {
    let configured = {
        let config = state.config();
        [config.wallpaper_dir(), config.video_dir()]
    };
    let mut roots: Vec<std::path::PathBuf> = Vec::new();
    for directory in configured {
        let path = std::path::PathBuf::from(directory);
        if !path.is_dir() || roots.iter().any(|root| path.starts_with(root)) {
            continue;
        }
        roots.retain(|root| !root.starts_with(&path));
        roots.push(path);
    }
    roots
}

fn watch_media_dirs<W: RootWatcher>(
    watcher: &mut W,
    roots: &[std::path::PathBuf],
) -> Vec<polling::PollingRoot> {
    let mut failed = Vec::new();
    for path in roots {
        match watcher.watch_root(path) {
            Ok(()) => log::info!("watching {}", path.display()),
            Err(error) => {
                log::warn!("watch {} failed: {error}", path.display());
                failed.push(polling::PollingRoot::new(path.clone(), error));
            }
        }
    }
    failed
}

fn watch_config_dir(watcher: &mut notify::RecommendedWatcher, cfg: &ConfigWatch) {
    use notify::{RecursiveMode, Watcher};

    for dir in cfg.dirs().filter(|dir| dir.is_dir()) {
        if let Err(err) = watcher.watch(dir, RecursiveMode::NonRecursive) {
            log::warn!("watch config dir {} failed: {err}", dir.display());
        }
    }
}

fn watch_theme_dirs(watcher: &mut notify::RecommendedWatcher) {
    use notify::{RecursiveMode, Watcher};

    let mut watched = Vec::new();
    for provider in ["caelestia", "dms", "end4"] {
        let Some(path) = skwd_wall_core::theme_provider::provider_path(provider) else {
            continue;
        };
        let Some(dir) = path.parent().filter(|dir| dir.is_dir()) else {
            continue;
        };
        if watched.iter().any(|path: &std::path::PathBuf| path == dir) {
            continue;
        }
        match watcher.watch(dir, RecursiveMode::NonRecursive) {
            Ok(()) => {
                watched.push(dir.to_path_buf());
                log::info!("watching theme provider directory {}", dir.display());
            }
            Err(err) => {
                log::warn!("watch theme provider directory {} failed: {err}", dir.display());
            }
        }
    }
}

fn recover_roots<W: RootWatcher>(
    watcher: &mut W,
    roots: &mut Vec<polling::PollingRoot>,
) -> Vec<std::path::PathBuf> {
    let mut recovered = Vec::new();
    roots.retain_mut(|root| match watcher.watch_root(&root.path) {
        Ok(()) => {
            recovered.push(root.path.clone());
            false
        }
        Err(error) => {
            root.reason = error;
            true
        }
    });
    recovered
}

struct PollCycle {
    roots: Vec<polling::PollingRoot>,
    recovery_watcher: Option<notify::RecommendedWatcher>,
    pending: Vec<std::path::PathBuf>,
    removed: Vec<std::path::PathBuf>,
    touched_roots: Vec<std::path::PathBuf>,
    force_full_scan: bool,
    recovered: Vec<std::path::PathBuf>,
}

fn run_poll_cycle(
    mut roots: Vec<polling::PollingRoot>,
    mut recovery_watcher: Option<notify::RecommendedWatcher>,
    tx: UnboundedSender<notify::Event>,
    cfg: &ConfigWatch,
    recover_auxiliary_watches: bool,
) -> PollCycle {
    let mut pending = Vec::new();
    let mut removed = Vec::new();
    let mut touched_roots = Vec::new();
    let mut force_full_scan = false;
    for root in &mut roots {
        match root.advance(polling::ENTRY_BUDGET_PER_ROOT) {
            polling::PollAdvance::Pending => {}
            polling::PollAdvance::Complete(delta) => {
                if delta.initial || !(delta.changed.is_empty() && delta.removed.is_empty()) {
                    touched_roots.push(root.path.clone());
                }
                force_full_scan |= delta.initial;
                pending.extend(delta.changed);
                removed.extend(delta.removed);
            }
            polling::PollAdvance::Failed(error) => {
                log::warn!("polling library root {} failed: {error}", root.path.display());
            }
        }
    }
    if recovery_watcher.is_none() {
        match create_watcher(tx) {
            Ok(mut watcher) => {
                if recover_auxiliary_watches {
                    watch_config_dir(&mut watcher, cfg);
                    watch_theme_dirs(&mut watcher);
                }
                recovery_watcher = Some(watcher);
            }
            Err(error) => {
                for root in &mut roots {
                    root.reason = format!("watcher recovery init failed: {error}");
                }
            }
        }
    }
    let recovered = recovery_watcher
        .as_mut()
        .map_or_else(Vec::new, |watcher| recover_roots(watcher, &mut roots));
    PollCycle {
        roots,
        recovery_watcher,
        pending,
        removed,
        touched_roots,
        force_full_scan,
        recovered,
    }
}

async fn poll_failed_roots(
    mut roots: Vec<polling::PollingRoot>,
    interval: std::time::Duration,
    tx: UnboundedSender<notify::Event>,
    cfg: ConfigWatch,
    recover_auxiliary_watches: bool,
    state: Arc<WallState>,
    publisher: Arc<EventHub>,
    workers: Arc<dyn MediaWorkerSupervisor>,
    stats: Arc<Stats>,
) {
    let mut recovery_watcher = None;
    loop {
        tokio::time::sleep(interval).await;
        let cycle_tx = tx.clone();
        let cycle_cfg = cfg.clone();
        let cycle = tokio::task::spawn_blocking(move || {
            run_poll_cycle(roots, recovery_watcher, cycle_tx, &cycle_cfg, recover_auxiliary_watches)
        })
        .await;
        let Ok(cycle) = cycle else {
            let detail = "library polling worker stopped unexpectedly";
            log::error!("{detail}");
            let failure_publisher = publisher.clone();
            let _ = tokio::task::spawn_blocking(move || {
                status::record_unavailable(failure_publisher.as_ref(), detail);
            })
            .await;
            publisher.publish(ev::WATCH_ERROR, json!({ "detail": detail, "mode": "unavailable" }));
            return;
        };
        roots = cycle.roots;
        recovery_watcher = cycle.recovery_watcher;
        if cycle.recovered.is_empty() {
            if cycle.touched_roots.is_empty() {
                status::record_polling(publisher.as_ref(), &roots, interval);
            } else {
                let request_id = next_scan_request_id();
                status::register_scan(
                    publisher.as_ref(),
                    &roots,
                    interval,
                    &request_id,
                    &cycle.touched_roots,
                    &[],
                );
                let launched = flush_watch_batch_async(
                    &state,
                    &publisher,
                    &workers,
                    &stats,
                    cycle.pending,
                    cycle.removed,
                    Some(request_id.clone()),
                    cycle.force_full_scan,
                )
                .await;
                if !launched {
                    status::complete_scan(publisher.as_ref(), &request_id);
                }
            }
        } else {
            let recovered = cycle
                .recovered
                .iter()
                .map(|path| path.to_string_lossy())
                .collect::<Vec<_>>()
                .join(", ");
            log::info!("native library watch recovered for {recovered}; requesting full rescan");
            let scan_workers = workers.clone();
            let scan_stats = stats.clone();
            let request_id = next_scan_request_id();
            let mut correlated_roots =
                roots.iter().map(|root| root.path.clone()).collect::<Vec<_>>();
            correlated_roots.extend(cycle.recovered.iter().cloned());
            correlated_roots.sort();
            correlated_roots.dedup();
            status::register_scan(
                publisher.as_ref(),
                &roots,
                interval,
                &request_id,
                &correlated_roots,
                &cycle.recovered,
            );
            let _ = tokio::task::spawn_blocking(move || {
                scan_stats.set_task("scanning");
                scan_workers.scan(&[], Some(&request_id));
            })
            .await;
        }
        if roots.is_empty() {
            let Some(watcher) = recovery_watcher else { return };
            hold_recovery_watcher(watcher).await;
            return;
        }
    }
}

async fn hold_recovery_watcher(_watcher: notify::RecommendedWatcher) {
    std::future::pending::<()>().await;
}

enum WatchStep {
    Event(notify::Event),
    Flush,
    Closed,
}

async fn next_step(
    rx: &mut UnboundedReceiver<notify::Event>,
    idle: bool,
    debounce: std::time::Duration,
) -> WatchStep {
    if idle {
        return match rx.recv().await {
            Some(event) => WatchStep::Event(event),
            None => WatchStep::Closed,
        };
    }
    match tokio::time::timeout(debounce, rx.recv()).await {
        Ok(Some(event)) => WatchStep::Event(event),
        Ok(None) => WatchStep::Closed,
        Err(_) => WatchStep::Flush,
    }
}

async fn watch_loop(
    mut watcher: Option<notify::RecommendedWatcher>,
    mut rx: UnboundedReceiver<notify::Event>,
    _keepalive: UnboundedSender<notify::Event>,
    mut cfg: ConfigWatch,
    ctx: crate::composition::context::Ctx,
    settings: session::WatchSettings,
) -> bool {
    let crate::composition::context::Ctx { state, events: publisher, workers, stats, .. } = ctx;
    let mut pending: Vec<std::path::PathBuf> = Vec::new();
    let mut removed: Vec<std::path::PathBuf> = Vec::new();
    let mut first_seen: Option<std::time::Instant> = None;
    loop {
        let idle = pending.is_empty() && removed.is_empty();
        let flush_now = match next_step(&mut rx, idle, WATCH_DEBOUNCE).await {
            WatchStep::Event(event) => absorb_and_hold(
                event,
                &mut cfg,
                watcher.as_mut(),
                &state,
                publisher.as_ref(),
                &mut pending,
                &mut removed,
                &mut first_seen,
            ),
            WatchStep::Flush => true,
            WatchStep::Closed => return false,
        };
        let current = session::WatchSettings::read(&state);
        if settings != current {
            let changed = std::array::from_fn::<_, 3, _>(|index| {
                settings.directories[index] != current.directories[index]
            });
            let invalidate_state = state.clone();
            let result = tokio::task::spawn_blocking(move || {
                invalidate_state.with_db(|connection| {
                    db::invalidate_source_mtimes(
                        connection,
                        changed[0],
                        changed[0] || changed[1],
                        changed[2],
                    )
                })
            })
            .await;
            if !matches!(result, Ok(Ok(_))) {
                log::warn!("failed to invalidate previous source metadata: {result:?}");
            }
            return true;
        }
        if flush_now {
            flush(&state, &publisher, &workers, &stats, &mut pending, &mut removed).await;
            first_seen = None;
        }
    }
}

async fn flush(
    state: &Arc<WallState>,
    publisher: &Arc<EventHub>,
    workers: &Arc<dyn MediaWorkerSupervisor>,
    stats: &Arc<Stats>,
    pending: &mut Vec<std::path::PathBuf>,
    removed: &mut Vec<std::path::PathBuf>,
) {
    let batch_pending = std::mem::take(pending);
    let batch_removed = std::mem::take(removed);
    flush_watch_batch_async(
        state,
        publisher,
        workers,
        stats,
        batch_pending,
        batch_removed,
        None,
        false,
    )
    .await;
}

async fn flush_watch_batch_async(
    state: &Arc<WallState>,
    publisher: &Arc<EventHub>,
    workers: &Arc<dyn MediaWorkerSupervisor>,
    stats: &Arc<Stats>,
    pending: Vec<std::path::PathBuf>,
    removed: Vec<std::path::PathBuf>,
    request_id: Option<String>,
    force_full_scan: bool,
) -> bool {
    let flush_state = Arc::clone(state);
    let flush_publisher = Arc::clone(publisher);
    let flush_workers = Arc::clone(workers);
    let flush_stats = Arc::clone(stats);
    tokio::task::spawn_blocking(move || {
        flush_watch_batch(
            &flush_state,
            flush_publisher.as_ref(),
            flush_workers.as_ref(),
            &flush_stats,
            pending,
            removed,
            request_id.as_deref(),
            force_full_scan,
        )
    })
    .await
    .unwrap_or(false)
}

fn absorb_and_hold(
    mut event: notify::Event,
    cfg: &mut ConfigWatch,
    watcher: Option<&mut notify::RecommendedWatcher>,
    state: &Arc<WallState>,
    publisher: &dyn EventPublisher,
    pending: &mut Vec<std::path::PathBuf>,
    removed: &mut Vec<std::path::PathBuf>,
    first_seen: &mut Option<std::time::Instant>,
) -> bool {
    import_theme_event(&event, state, publisher);
    event.paths.retain(|path| skwd_wall_core::theme_provider::provider_for_path(path).is_none());
    if absorb_watch_event(event, cfg, pending, removed) {
        cfg.refresh(watcher);
        let backdrop_before = super::overview_backdrop::settings(&state.config());
        let (lock_screen_before, semantic_before) = {
            let config = state.config();
            (
                (
                    config.plasma_lock_screen_mode(),
                    config.plasma_lock_screen_image(),
                    config.plasma_lock_screen_live(),
                ),
                (config.semantic_manifest(), config.semantic_index_profile()),
            )
        };
        state.reload_config();
        let backdrop_config = state.config().clone();
        if backdrop_before != super::overview_backdrop::settings(&backdrop_config) {
            tokio::task::spawn_blocking(move || {
                if let Err(error) = super::overview_backdrop::refresh_from_disk(&backdrop_config) {
                    log::warn!("overview-backdrop: {error}");
                }
            });
        }
        let (lock_screen_after, semantic_after) = {
            let config = state.config();
            (
                (
                    config.plasma_lock_screen_mode(),
                    config.plasma_lock_screen_image(),
                    config.plasma_lock_screen_live(),
                ),
                (config.semantic_manifest(), config.semantic_index_profile()),
            )
        };
        if lock_screen_before != lock_screen_after {
            crate::infrastructure::lock_screen::request_sync(state);
        }
        if semantic_before != semantic_after {
            crate::infrastructure::semantic_index::request_refresh();
        }
        publisher.publish(ev::CONFIG_CHANGED, json!({}));
        crate::infrastructure::power::request_refresh();
        crate::infrastructure::playback::refresh();
        crate::composition::runtime::rotation::wake();
        crate::infrastructure::vitals::wake();
    }
    if first_seen.is_none() && !(pending.is_empty() && removed.is_empty()) {
        *first_seen = Some(std::time::Instant::now());
    }
    first_seen.is_some_and(|since| hold_exceeded(since.elapsed(), WATCH_MAX_HOLD))
}

fn theme_event_provider(event: &notify::Event) -> Option<&'static str> {
    use notify::EventKind;

    if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
        return None;
    }
    event.paths.iter().find_map(|path| skwd_wall_core::theme_provider::provider_for_path(path))
}

fn import_theme_event(
    event: &notify::Event,
    state: &Arc<WallState>,
    publisher: &dyn EventPublisher,
) {
    let Some(provider) = theme_event_provider(event) else {
        return;
    };
    let config = state.config().clone();
    if config.theme().authority() != provider {
        return;
    }
    let dark = config.theme().mode() != "light";
    if skwd_wall_core::theme_provider::import(&config, provider, dark) {
        let source = skwd_wall_core::theme_provider::provider_path(provider)
            .map_or_else(String::new, |path| path.to_string_lossy().into_owned());
        publisher.publish(
            ev::THEME_DONE,
            json!({
                "source": source,
                "ok": true,
                "backend": provider,
                "requested": provider,
                "external": true,
            }),
        );
    }
}

fn absorb_watch_event(
    event: notify::Event,
    cfg: &ConfigWatch,
    pending: &mut Vec<std::path::PathBuf>,
    removed: &mut Vec<std::path::PathBuf>,
) -> bool {
    use notify::EventKind;

    match event.kind {
        EventKind::Remove(_) => {
            split_config_events(event.paths, cfg, removed);
            false
        }
        EventKind::Create(_) | EventKind::Modify(_) => {
            split_config_events(event.paths, cfg, pending)
        }
        _ => false,
    }
}

pub(crate) fn handle_remove(
    state: &Arc<WallState>,
    publisher: &dyn EventPublisher,
    path: &std::path::Path,
) -> bool {
    let (wdir, vdir) = {
        let cfg = state.config();
        (cfg.wallpaper_dir(), cfg.video_dir())
    };
    let Some(key) = skwd_wall_core::paths::key_for_path(path, &wdir, &vdir) else {
        return true;
    };
    if let Some(rel) = key.strip_prefix("static:") {
        let _ = std::fs::remove_file(skwd_wall_core::paths::image_thumb(rel));
        let _ = std::fs::remove_file(skwd_wall_core::paths::image_thumb_sm(rel));
    } else if let Some(rel) = key.strip_prefix("video:") {
        let _ = std::fs::remove_file(skwd_wall_core::paths::video_thumb(rel));
        let _ = std::fs::remove_file(skwd_wall_core::paths::video_thumb_sm(rel));
    }
    match state.with_db(|conn| db::delete_entries(conn, std::slice::from_ref(&key))) {
        Ok(count) => {
            if count > 0 {
                log::info!("removed {key}");
                publisher.publish(ev::REMOVED, json!({ "key": key }));
                crate::infrastructure::semantic_index::request_refresh();
            }
            true
        }
        Err(error) => {
            log::warn!("could not remove {key} from the library database: {error}");
            false
        }
    }
}

mod tests;
