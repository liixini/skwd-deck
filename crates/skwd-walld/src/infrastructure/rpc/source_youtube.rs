#[allow(clippy::wildcard_imports)]
use super::common::*;

use super::source::{require_enabled, respond_exists, spawn_download};

pub(super) fn youtube_source_download(ctx: &Ctx, request: &Request) -> Response {
    let Ctx { state, events, workers, stats, .. } = ctx;
    if let Some(response) = require_enabled(state, request, stats, "youtube") {
        return response;
    }
    let id = request.str_param("id", "").to_string();
    if !sources::youtube::safe_id(&id) {
        return crate::infrastructure::rpc::fail_msg(
            stats,
            request.id,
            -32602,
            "missing or invalid youtube id",
        );
    }
    let (video_dir, max_height, max_minutes) = {
        let config = state.config();
        (config.video_dir(), config.youtube_max_height(), config.youtube_max_minutes())
    };
    let start = request.opt_i64("start").unwrap_or(0).max(0) as u64;
    let duration = match request.opt_i64("dur") {
        Some(duration) if duration > 0 => duration as u64,
        Some(_) => 0,
        None => max_minutes.saturating_mul(60),
    };
    if let Some(existing) = crate::infrastructure::youtube_download::finished_video(&video_dir, &id)
    {
        return respond_exists(request.id, events.as_ref(), &id, &existing);
    }
    let workers = Arc::clone(workers);
    let download_id = id.clone();
    spawn_download(
        request.id,
        &id,
        events,
        stats,
        &crate::infrastructure::dlqueue::VIDEOS,
        format!("youtube:{id}"),
        move |events, _stats, slot| {
            let publisher: Arc<dyn EventPublisher> = events.clone();
            let succeeded = crate::infrastructure::youtube_download::run_download(
                &download_id,
                &video_dir,
                max_height,
                start,
                duration,
                &publisher,
            );
            drop(slot);
            if succeeded {
                workers.scan(&[], None);
            }
        },
    )
}
