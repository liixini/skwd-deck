#[allow(clippy::wildcard_imports)]
use super::common::*;

use super::source::{InflightGuard, handle_remote_preview, require_enabled, require_id_url};

pub(super) fn steam_search(ctx: &Ctx, request: &Request) -> Response {
    let Ctx { state, workers, stats, .. } = ctx;
    if let Some(response) = require_enabled(state, request, stats, "steam") {
        return response;
    }
    let string_array = |key: &str| {
        request
            .params
            .get(key)
            .and_then(serde_json::Value::as_array)
            .map(|values| {
                values.iter().filter_map(|value| value.as_str().map(str::to_string)).collect()
            })
            .unwrap_or_default()
    };
    let requested_query_type = request.opt_i64("query_type").unwrap_or(3);
    let query_type = match requested_query_type {
        0 | 1 | 3 | 9 | 21 => requested_query_type as u32,
        _ => 3,
    };
    let params = steam::SearchParams {
        query: request.str_param("query", "").to_string(),
        query_type,
        days: request.opt_i64("days").unwrap_or(7).max(0) as u32,
        tags: string_array("tags"),
        excluded_tags: string_array("excluded_tags"),
        page: request.opt_i64("page").unwrap_or(1).max(1) as u32,
        numperpage: request.opt_i64("numperpage").unwrap_or(30).clamp(1, 50) as u32,
    };
    let (backend, api_key, wallpaper_engine_dir) = {
        let config = state.config();
        (config.steam_backend(), config.steam_api_key(), config.we_dir())
    };
    let page = match if backend == "steam" {
        steam_helper_search(&params)
    } else {
        steam::search(&params, &api_key)
    } {
        Ok(page) => page,
        Err(error) => {
            return crate::infrastructure::rpc::fail_msg(stats, request.id, -1, error.to_string());
        }
    };
    let local = steam::downloaded_ids(&wallpaper_engine_dir);
    let items: Vec<wall_proto::sources::ListItem> = page
        .results
        .iter()
        .map(|result| {
            let thumb_path = skwd_wall_core::paths::remote_thumb("steam", &result.id);
            wall_proto::sources::ListItem {
                id: result.id.clone(),
                full_url: result.preview_url.clone(),
                thumb_url: result.preview_url.clone(),
                thumb_path: thumb_path.to_string_lossy().into_owned(),
                file_size: result.file_size,
                category: result.tags.clone(),
                title: result.title.clone(),
                downloaded: local.contains(&result.id),
                ..wall_proto::sources::ListItem::default()
            }
        })
        .collect();
    let jobs: Vec<(String, String)> = page
        .results
        .iter()
        .filter(|result| !result.preview_url.is_empty())
        .map(|result| (result.id.clone(), result.preview_url.clone()))
        .collect();
    workers.remote_thumbnails("steam", &jobs);
    Response::ok(
        request.id,
        json!(wall_proto::sources::ListResult {
            generation: request.opt_u64("generation"),
            results: items,
            last_page: page.last_page,
            current_page: page.current_page,
            next_cursor: None,
        }),
    )
}

pub(super) fn steam_preview(ctx: &Ctx, request: &Request) -> Response {
    let Ctx { state, events, stats, .. } = ctx;
    if let Some(response) = require_enabled(state, request, stats, "steam") {
        return response;
    }
    let (id, url) = match require_id_url(request, stats, false) {
        Ok(pair) => pair,
        Err(response) => return response,
    };
    let destination =
        skwd_wall_core::paths::remote_preview("steam", &id, steam::ext_from_url(&url));
    handle_remote_preview(request.id, &id, url, destination, events, |url| {
        crate::infrastructure::http::require_source("steam", url)
    })
}

pub(super) fn steam_download(ctx: &Ctx, request: &Request) -> Response {
    let Ctx { state, events, workers, stats, .. } = ctx;
    if let Some(response) = require_enabled(state, request, stats, "steam") {
        return response;
    }
    let id = request.str_param("id", "").to_string();
    if id.is_empty() || !id.chars().all(|ch| ch.is_ascii_digit()) {
        return crate::infrastructure::rpc::fail_msg(
            stats,
            request.id,
            -32602,
            "missing/invalid workshop id",
        );
    }
    let wallpaper_engine_dir = state.config().we_dir();
    if wallpaper_engine_dir.join(&id).is_dir() {
        steam_dl_event(events.as_ref(), &id, "done", 1.0);
        return Response::ok(request.id, json!({"id": id, "status": "exists"}));
    }
    if !steam_inflight_begin(&id) {
        return Response::ok(request.id, json!({"id": id, "status": "in_progress"}));
    }
    let guard = InflightGuard(id.clone(), steam_inflight_end);
    let Some(reservation) = crate::infrastructure::dlqueue::VIDEOS.try_reserve() else {
        drop(guard);
        return Response::err(request.id, -32000, "download queue is full; retry shortly");
    };
    let backend = state.config().steam_backend();
    let username = state.config().steam_username();
    let install_root = state.config().steam_install_root().to_string_lossy().into_owned();
    let events = Arc::clone(events);
    let workers = Arc::clone(workers);
    let download_id = id.clone();
    match thread::Builder::new().name("skwd-steam-download".into()).spawn(move || {
        let _guard = guard;
        let slot = reservation.acquire(|ahead| {
            events.publish(
                ev::DOWNLOAD,
                wall_proto::DownloadEvent {
                    progress: Some(0.0),
                    message: Some(crate::infrastructure::dlqueue::queue_label(ahead)),
                    ..wall_proto::DownloadEvent::new(&download_id, wall_proto::dl_status::QUEUED)
                }
                .to_value(),
            );
        });
        steam_dl_event(events.as_ref(), &download_id, "downloading", 0.0);
        let ids = [download_id.clone()];
        let succeeded = if backend == "steamcmd" {
            run_steamcmd_download(
                events.as_ref(),
                &username,
                &install_root,
                &wallpaper_engine_dir,
                &ids,
            )
        } else {
            run_steamworks_download(events.as_ref(), &wallpaper_engine_dir, &ids)
        };
        drop(slot);
        if succeeded {
            workers.scan(&[], None);
        }
    }) {
        Ok(_) => Response::ok(request.id, json!({"id": id, "status": "started"})),
        Err(error) => {
            stats.error();
            Response::err(request.id, -1, format!("failed to start Steam download: {error}"))
        }
    }
}
