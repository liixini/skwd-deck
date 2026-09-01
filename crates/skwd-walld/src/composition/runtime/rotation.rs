use std::sync::Arc;
use std::time::Duration;

use skwd_wall_core::backend::wallpaper::WallpaperApplication;
use skwd_wall_core::{WallState, db};
use tokio::sync::Notify;

use crate::backend::events::EventPublisher;
use crate::backend::history::{ApplySource, HistoryRepository};
use crate::composition::apply::apply_core;
use crate::composition::runtime::playlist;
use crate::infrastructure::stats::Stats;
use crate::infrastructure::wake::wake_or_timeout;

static WAKE: Notify = Notify::const_new();

pub(crate) fn wake() {
    WAKE.notify_one();
}

pub(crate) fn start_rotation(ctx: crate::composition::context::Ctx) {
    tokio::spawn(rotation_loop(ctx));
}

async fn rotation_loop(ctx: crate::composition::context::Ctx) {
    let mut last: Option<String> = None;
    loop {
        let poll_state = Arc::clone(&ctx.state);
        let Ok((rotate, interval, types, fav, playlist_active)) =
            tokio::task::spawn_blocking(move || {
                poll_state.reload_config();
                let (rotate, interval, types, fav) = {
                    let cfg = poll_state.config();
                    (
                        cfg.random_rotate(),
                        cfg.random_interval(),
                        cfg.random_types(),
                        cfg.random_favourites_only(),
                    )
                };
                (rotate, interval, types, fav, playlist::assignments_active(&poll_state))
            })
            .await
        else {
            break;
        };
        if !rotate || types.is_empty() || playlist_active {
            wake_or_timeout(&WAKE, crate::composition::bootstrap::IDLE_RECHECK).await;
            continue;
        }
        if !wake_or_timeout(&WAKE, Duration::from_secs(interval)).await {
            continue;
        }
        let task = ctx.clone();
        let exclude = last.clone();
        let rotated = tokio::task::spawn_blocking(move || {
            let type_refs: Vec<&str> = types.iter().map(String::as_str).collect();
            rotate_once(
                &task.state,
                task.wallpaper.as_ref(),
                task.history.as_ref(),
                task.events.as_ref(),
                &task.stats,
                &type_refs,
                fav,
                exclude.as_deref(),
                ApplySource::Rotation,
            )
        })
        .await
        .ok()
        .flatten();
        if let Some(name) = rotated {
            last = Some(name);
        }
    }
}

pub(crate) fn rotate_once(
    state: &Arc<WallState>,
    application: &dyn WallpaperApplication,
    history: &dyn HistoryRepository,
    publisher: &dyn EventPublisher,
    stats: &Stats,
    types: &[&str],
    fav: bool,
    exclude: Option<&str>,
    source: ApplySource,
) -> Option<String> {
    let (_key, ty, name, video_file, we_id) =
        state.with_db(|conn| db::random_pick(conn, exclude, types, fav)).ok().flatten()?;
    let (path, mute, volume) = {
        let cfg = state.config();
        let path = match ty.as_str() {
            wall_proto::kind::STATIC => std::path::Path::new(&cfg.wallpaper_dir())
                .join(&name)
                .to_string_lossy()
                .into_owned(),
            wall_proto::kind::VIDEO => video_file,
            _ => String::new(),
        };
        let (mute, volume) = skwd_wall_core::audio::resolve_defaults(
            &cfg.cache_dir(),
            "*",
            cfg.renderer().mute(),
            cfg.renderer().volume(),
        );
        (path, mute, volume)
    };
    stats.set_task("rotating");
    let res = apply_core(
        state,
        application,
        history,
        publisher,
        stats,
        &ty,
        &path,
        &we_id,
        mute,
        volume,
        source,
        "*",
        true,
        false,
        None,
        None,
    );
    stats.set_task("idle");
    match res {
        Ok(_) => {
            log::info!("rotated to {name}");
            Some(name)
        }
        Err(err) => {
            log::warn!("rotate failed for {name}: {err}");
            None
        }
    }
}

mod tests;
