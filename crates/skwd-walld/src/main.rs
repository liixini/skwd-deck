use std::sync::Arc;

use skwd_wall_core::WallState;
use skwd_wall_core::backend::renderers::RendererSupervision;
use skwd_wall_core::backend::wallpaper::WallpaperApplication;
use skwd_wall_core::infrastructure::wallpaper::CoreWallpaperApplication;

use crate::backend::events::EventPublisher;
use crate::backend::history::HistoryRepository;
use crate::backend::workers::MediaWorkerSupervisor;
use crate::composition::context::Ctx;
use crate::infrastructure::events::EventHub;
use crate::infrastructure::history::FileHistoryRepository;
use crate::infrastructure::processes::ProcessSupervisor;
use crate::infrastructure::stats::Stats;
use crate::infrastructure::{doctor, logging, platform};

#[cfg(feature = "obs-heap")]
#[global_allocator]
static GLOBAL: skwd_wall_core::countalloc::Counting<std::alloc::System> =
    skwd_wall_core::countalloc::Counting(std::alloc::System);

mod backend;
mod composition;
mod domain;
mod infrastructure;

fn main() -> anyhow::Result<()> {
    platform::tune_allocator();

    if std::env::args().any(|argument| argument == "--version" || argument == "-V") {
        println!("skwd-walld {}", skwd_wall_core::version());
        return Ok(());
    }
    if std::env::args().any(|argument| argument == "--diag" || argument == "diag") {
        return platform::run_diag_client();
    }
    if std::env::args().any(|argument| argument == "--doctor" || argument == "doctor") {
        let json = std::env::args().any(|argument| argument == "--json");
        std::process::exit(doctor::run(json));
    }
    if std::env::args().any(|argument| argument == "--bug-report" || argument == "bug-report") {
        std::process::exit(doctor::bug_report());
    }
    if std::env::args().any(|argument| argument == "--image-optimize-worker") {
        let debug = platform::debug_enabled();
        logging::init_logging(if debug { "debug" } else { "info" });
        return infrastructure::image_optimization::run_worker();
    }

    let debug = platform::debug_enabled();
    logging::init_logging(if debug { "debug" } else { "info" });
    if std::env::args().any(|argument| argument == "--wait-for-session") {
        infrastructure::session_env::wait_for_wayland();
    }
    log::info!(
        "skwd-walld {} | desktop={} wayland={} runtime={}",
        skwd_wall_core::version(),
        std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "?".into()),
        std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "?".into()),
        std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "?".into()),
    );

    let state = Arc::new(WallState::open()?);
    let config = state.config_store();
    let database = state.database();
    let renderer_supervisor = state.renderer_supervisor();
    let renderers: Arc<dyn RendererSupervision> = renderer_supervisor;
    let wallpaper: Arc<dyn WallpaperApplication> =
        Arc::new(CoreWallpaperApplication::new(Arc::clone(&state)));
    let history: Arc<dyn HistoryRepository> =
        Arc::new(FileHistoryRepository::new(config.read().cache_dir()));
    let stats = Arc::new(Stats::new());
    let events = Arc::new(EventHub::new(Arc::clone(&stats)));
    let tasks = Arc::new(crate::infrastructure::tasks::TaskRegistry::new(Arc::clone(&events)));
    let publisher: Arc<dyn EventPublisher> = events.clone();
    let workers: Arc<dyn MediaWorkerSupervisor> = Arc::new(ProcessSupervisor::new(
        Arc::clone(&state),
        Arc::clone(&config),
        Arc::clone(&database),
        publisher,
        Arc::clone(&tasks),
        Arc::clone(&stats),
        debug,
    ));
    let ctx = Ctx {
        state: Arc::clone(&state),
        config,
        database,
        renderers,
        wallpaper,
        history,
        events,
        workers,
        stats: Arc::clone(&stats),
        tasks,
    };

    if debug {
        log::info!(
            "skwd-walld {} debug mode\n{}{}",
            skwd_wall_core::version(),
            skwd_wall_core::diag::env_report(),
            skwd_wall_core::diag::config_report(&state.config())
        );
    }

    let Some((listener, socket_path)) = platform::bind_socket()? else {
        return Ok(());
    };
    log::info!("skwd-walld {} listening on {}", skwd_wall_core::version(), socket_path.display());

    let runtime = composition::bootstrap::start_services(&ctx, debug)?;
    platform::run_server(&runtime, listener, ctx)
}

pub(crate) mod testenv;

mod tests;
