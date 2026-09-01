use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use skwd_wall_core::backend::renderers::RendererSupervision;
use skwd_wall_core::lock;

use crate::backend::history::ApplySource;
use crate::composition::context::Ctx;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
const VRAM_CACHE_TTL: Duration = Duration::from_secs(5);
pub(crate) const IDLE_RECHECK: Duration = Duration::from_secs(3600);

static VRAM_CACHE: Mutex<Option<(Instant, u64)>> = Mutex::new(None);

pub(crate) fn start_services(ctx: &Ctx, debug: bool) -> anyhow::Result<tokio::runtime::Runtime> {
    let runtime = crate::infrastructure::platform::build_runtime()?;
    crate::infrastructure::proc::register_runtime(runtime.handle().clone());
    let services = runtime.enter();
    let state = &ctx.state;
    crate::infrastructure::semantic_index::start(
        Arc::clone(&ctx.config),
        Arc::clone(&ctx.database),
        Arc::clone(&ctx.events),
        Arc::clone(&ctx.tasks),
    );
    ensure_media_dirs(&state.config());
    crate::infrastructure::templates_seed::seed(&state.config().theme().templates_dir());
    ctx.stats.set_task("scanning");
    ctx.tasks.update(wall_proto::TaskStatus::running("scan", "scan", "Scanning wallpapers"));
    ctx.workers.scan(&[], None);
    crate::infrastructure::reap::kill_stale_renderers(&{
        let config = state.config();
        [config.cache_dir() + "/video-opt", config.video_dir()]
    });
    crate::infrastructure::video_optimization::retire_general_cache(state);
    for sink in &skwd_wall_core::theme_sink::SINKS {
        if let Some(restore) = sink.restore_stale {
            restore(&state.config());
        }
    }
    crate::infrastructure::effects_preview::sweep_effects_previews(
        &skwd_wall_core::paths::cache_dir().join("effects-preview"),
    );
    crate::infrastructure::theme_worker::start_theme_worker(ctx.clone());
    // Must publish the power snapshot before restore reads renderer policy.
    crate::infrastructure::power::start(ctx.clone());
    crate::infrastructure::lock_screen::request_sync(state);
    spawn_restore(ctx);
    crate::infrastructure::watcher::start_watcher(ctx.clone());
    crate::composition::runtime::rotation::start_rotation(ctx.clone());
    crate::composition::runtime::schedule::start_scheduler(ctx.clone());
    crate::composition::runtime::playlist::start(ctx.clone());
    crate::infrastructure::workspaces::start(ctx.clone());
    crate::infrastructure::vitals::start(ctx.clone());
    crate::infrastructure::hotplug::start_output_watch(ctx.clone());

    if debug {
        spawn_heartbeat(ctx);
    }
    drop(services);
    Ok(runtime)
}

fn spawn_restore(ctx: &Ctx) {
    let Ctx { state, wallpaper, history, events, stats, .. } = ctx;
    let restore_state = Arc::clone(state);
    let application = Arc::clone(wallpaper);
    let history = Arc::clone(history);
    let events = Arc::clone(events);
    let restore_stats = Arc::clone(stats);
    let restore_wallpaper = state.config().restore_on_startup();
    let restore_backdrop = state.config().niri_overview_backdrop();
    thread::spawn(move || {
        if restore_wallpaper {
            crate::infrastructure::hotplug::restore_last(
                &restore_state,
                application.as_ref(),
                history.as_ref(),
                events.as_ref(),
                &restore_stats,
                ApplySource::Restore,
            );
        }
        if restore_backdrop {
            crate::infrastructure::overview_backdrop::refresh_from_disk(&restore_state.config());
        }
    });
}

fn spawn_heartbeat(ctx: &Ctx) {
    let Ctx { state, renderers, events, stats, .. } = ctx;
    let heartbeat_state = Arc::clone(state);
    let renderers = Arc::clone(renderers);
    let events = Arc::clone(events);
    let stats = Arc::clone(stats);
    thread::spawn(move || {
        loop {
            thread::sleep(HEARTBEAT_INTERVAL);
            let subscriber_count = events.subscriber_count();
            log::info!(
                "\n{}",
                stats.banner(
                    skwd_wall_core::diag::rss_mb(),
                    renderers.wallpaper_rss_mb(),
                    renderers.wallpaper_count(),
                    renderers.scene_rss_mb(),
                    renderers.scene_count(),
                    heartbeat_state.scanner().scanner_rss_mb(),
                    vram_mb(renderers.as_ref()),
                    subscriber_count,
                )
            );
        }
    });
}

pub(crate) fn vram_mb(renderers: &dyn RendererSupervision) -> u64 {
    let mut pids = renderers.scene_pids();
    pids.extend(renderers.wallpaper_pids());
    if pids.is_empty() {
        return 0;
    }
    let mut cache = lock(&VRAM_CACHE);
    if let Some((cached_at, megabytes)) = *cache
        && cached_at.elapsed() < VRAM_CACHE_TTL
    {
        return megabytes;
    }
    let megabytes = skwd_wall_core::diag::vram_mb_for(&pids);
    *cache = Some((Instant::now(), megabytes));
    megabytes
}

fn ensure_media_dirs(config: &skwd_wall_core::config::Config) {
    for directory in [config.wallpaper_dir(), config.video_dir()] {
        if directory.is_empty() || std::path::Path::new(&directory).is_dir() {
            continue;
        }
        match std::fs::create_dir_all(&directory) {
            Ok(()) => log::info!("created wallpaper dir {directory}"),
            Err(error) => log::warn!("could not create wallpaper dir {directory}: {error}"),
        }
    }
    let templates = config.theme().templates_dir();
    if !templates.is_dir()
        && let Err(error) = std::fs::create_dir_all(&templates)
    {
        log::warn!("could not create matugen templates dir {}: {error}", templates.display());
    }
}
