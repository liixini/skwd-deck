use super::{
    Arc, WallState, create_watcher, flush_watch_batch_async, media_roots, poll_failed_roots,
    polling, record_polling_fallback, record_watch_failure, status, watch_config_dir, watch_loop,
    watch_media_dirs, watch_theme_dirs,
};
use crate::composition::context::Ctx;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct WatchSettings {
    pub(super) directories: [std::path::PathBuf; 3],
    polling_enabled: bool,
    interval_seconds: u64,
}

impl WatchSettings {
    pub(super) fn read(state: &Arc<WallState>) -> Self {
        let config = state.config();
        Self {
            directories: [
                config.wallpaper_dir().into(),
                config.video_dir().into(),
                config.we_dir(),
            ],
            polling_enabled: config.library_polling_fallback(),
            interval_seconds: config.library_polling_interval_seconds(),
        }
    }
}

struct Session {
    main: tokio::task::JoinHandle<bool>,
    recovery: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.main.abort();
        if let Some(recovery) = &self.recovery {
            recovery.abort();
        }
    }
}

pub(crate) fn start_watcher(ctx: Ctx) {
    tokio::spawn(async move {
        let mut rescan = false;
        loop {
            let install_ctx = ctx.clone();
            let runtime = tokio::runtime::Handle::current();
            let session = tokio::task::spawn_blocking(move || {
                let _entered = runtime.enter();
                install(install_ctx)
            })
            .await;
            let Ok(Some(mut session)) = session else {
                return;
            };
            if rescan {
                log::info!(
                    "library source configuration changed; watches replaced, requesting full rescan"
                );
                flush_watch_batch_async(
                    &ctx.state,
                    &ctx.events,
                    &ctx.workers,
                    &ctx.stats,
                    Vec::new(),
                    Vec::new(),
                    None,
                    true,
                )
                .await;
            }
            let restart = (&mut session.main).await.unwrap_or(false);
            drop(session);
            if !restart {
                return;
            }
            rescan = true;
        }
    });
}

fn install(ctx: Ctx) -> Option<Session> {
    let settings = WatchSettings::read(&ctx.state);
    let loop_ctx = ctx.clone();
    let mut recovery = None;
    let crate::composition::context::Ctx { state, events, workers, stats, .. } = ctx;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<notify::Event>();
    let roots = media_roots(&state);
    let (polling_enabled, poll_interval) = {
        let config = state.config();
        (
            config.library_polling_fallback(),
            std::time::Duration::from_secs(config.library_polling_interval_seconds()),
        )
    };
    let cfg_path = skwd_wall_core::config::config_path();
    let cfg_path_for_loop = cfg_path.clone();
    let tx_for_loop = tx.clone();
    let mut watcher = match create_watcher(tx.clone()) {
        Ok(watcher) => watcher,
        Err(error) => {
            let detail = format!("watcher init failed: {error}");
            if !polling_enabled || roots.is_empty() {
                record_watch_failure(events.as_ref(), &detail);
                return None;
            }
            let polling_roots = roots
                .iter()
                .cloned()
                .map(|path| polling::PollingRoot::new(path, detail.clone()))
                .collect::<Vec<_>>();
            record_polling_fallback(events.as_ref(), &polling_roots, poll_interval);
            recovery = Some(tokio::spawn(poll_failed_roots(
                polling_roots,
                poll_interval,
                tx,
                cfg_path,
                true,
                state,
                events,
                workers,
                stats,
            )));
            return Some(Session {
                main: tokio::spawn(watch_loop(
                    None,
                    rx,
                    tx_for_loop,
                    cfg_path_for_loop,
                    loop_ctx,
                    settings,
                )),
                recovery,
            });
        }
    };

    let polling_roots = watch_media_dirs(&mut watcher, &roots);
    watch_config_dir(&mut watcher, &cfg_path);
    watch_theme_dirs(&mut watcher);
    if polling_roots.is_empty() {
        status::record_native(events.as_ref(), "native library watch is active", false);
    } else if polling_enabled {
        record_polling_fallback(events.as_ref(), &polling_roots, poll_interval);
        recovery = Some(tokio::spawn(poll_failed_roots(
            polling_roots,
            poll_interval,
            tx.clone(),
            cfg_path.clone(),
            false,
            state.clone(),
            events.clone(),
            workers.clone(),
            stats.clone(),
        )));
    } else {
        let detail = polling_roots
            .iter()
            .map(|root| format!("{}: {}", root.path.display(), root.reason))
            .collect::<Vec<_>>()
            .join("; ");
        record_watch_failure(events.as_ref(), &detail);
    }

    let _ = tx.send(
        notify::Event::new(notify::EventKind::Modify(notify::event::ModifyKind::Any))
            .add_path(cfg_path.clone()),
    );
    Some(Session {
        main: tokio::spawn(watch_loop(Some(watcher), rx, tx, cfg_path, loop_ctx, settings)),
        recovery,
    })
}
