#[allow(clippy::wildcard_imports)]
use super::common::*;

use crate::infrastructure::events::EventHub;

use super::source::{handle_remote_preview, require_id_url, respond_exists, spawn_download};

pub(super) fn wallhaven_search(ctx: &Ctx, request: &Request) -> Response {
    let Ctx { state, workers, stats, .. } = ctx;
    let params = wallhaven::SearchParams {
        query: request.str_param("query", "").to_string(),
        categories: request.str_param("categories", "111").to_string(),
        purity: request.str_param("purity", "100").to_string(),
        sorting: request.str_param("sorting", "toplist").to_string(),
        order: request.str_param("order", "desc").to_string(),
        top_range: request.str_param("topRange", "1M").to_string(),
        atleast: request.str_param("atleast", "").to_string(),
        resolutions: request.str_param("resolutions", "").to_string(),
        ratios: request.str_param("ratios", "").to_string(),
        colors: request.str_param("colors", "").to_string(),
        page: request.opt_i64("page").unwrap_or(1).max(1) as u32,
    };
    let api_key = state.config().wallhaven_api_key();
    let collection = request.str_param("collection", "").to_string();
    let result = if collection.is_empty() {
        wallhaven::search(&params, &api_key)
    } else {
        let username = state.config().wallhaven_username();
        wallhaven::collection_page(&username, &collection, params.page, &api_key)
    };
    let mut page = match result {
        Ok(page) => page,
        Err(error) => {
            return crate::infrastructure::rpc::fail_msg(stats, request.id, -1, error.to_string());
        }
    };
    let at_most = request.str_param("atmost", "").to_string();
    if !at_most.is_empty() {
        page.results.retain(|result| wallhaven::within_max(&result.resolution, &at_most));
    }
    let wallpaper_dir = state.config().wallpaper_dir();
    let local_ids = wallhaven::library_ids(&wallpaper_dir);
    let items: Vec<wall_proto::sources::ListItem> = page
        .results
        .iter()
        .map(|result| {
            let thumb_path = skwd_wall_core::paths::remote_thumb("wallhaven", &result.id);
            wall_proto::sources::ListItem {
                id: result.id.clone(),
                full_url: result.full_url.clone(),
                thumb_url: result.thumb_large.clone(),
                thumb_path: thumb_path.to_string_lossy().into_owned(),
                resolution: result.resolution.clone(),
                file_size: result.file_size,
                purity: result.purity.clone(),
                category: result.category.clone(),
                downloaded: local_ids.contains(&result.id),
                ..wall_proto::sources::ListItem::default()
            }
        })
        .collect();
    let jobs: Vec<(String, String)> =
        page.results.iter().map(|result| (result.id.clone(), result.thumb_large.clone())).collect();
    workers.remote_thumbnails("wallhaven", &jobs);
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

pub(super) fn wallhaven_collections(
    state: &Arc<WallState>,
    request: &Request,
    stats: &Arc<Stats>,
) -> Response {
    let username = state.config().wallhaven_username();
    if username.is_empty() {
        return Response::err(request.id, -1, "set your wallhaven username in settings first");
    }
    let api_key = state.config().wallhaven_api_key();
    match wallhaven::collections(&username, &api_key) {
        Ok(collections) => {
            let items: Vec<serde_json::Value> = collections
                .iter()
                .map(|collection| {
                    json!({
                        "id": collection.id,
                        "label": collection.label,
                        "count": collection.count
                    })
                })
                .collect();
            Response::ok(request.id, json!({ "collections": items }))
        }
        Err(error) => fail(stats, request.id, error),
    }
}

pub(super) fn wallhaven_preview(
    request: &Request,
    events: &Arc<EventHub>,
    stats: &Arc<Stats>,
) -> Response {
    let (id, url) = match require_id_url(request, stats, false) {
        Ok(pair) => pair,
        Err(response) => return response,
    };
    let destination =
        skwd_wall_core::paths::remote_preview("wallhaven-full", &id, wallhaven::ext_from_url(&url));
    handle_remote_preview(
        request.id,
        &id,
        url,
        destination,
        events,
        crate::infrastructure::http::require_wallhaven,
    )
}

pub(super) fn wallhaven_download(ctx: &Ctx, request: &Request) -> Response {
    let Ctx { state, events, stats, .. } = ctx;
    let (id, url) = match require_id_url(request, stats, true) {
        Ok(pair) => pair,
        Err(response) => return response,
    };
    let wallpaper_dir = state.config().wallpaper_dir();
    if let Some(existing) = wallhaven::library_path(&wallpaper_dir, &id) {
        return respond_exists(request.id, events.as_ref(), &id, &existing);
    }
    let download_id = id.clone();
    spawn_download(
        request.id,
        &id,
        events,
        stats,
        &crate::infrastructure::dlqueue::IMAGES,
        format!("wallhaven:{id}"),
        move |events, _stats, slot| {
            events.publish(
                ev::DOWNLOAD,
                wall_proto::DownloadEvent::new(&download_id, wall_proto::dl_status::DOWNLOADING)
                    .to_value(),
            );
            wallhaven_fetch(events.as_ref(), &url, &wallpaper_dir, &download_id);
            drop(slot);
        },
    )
}

pub(super) fn wallhaven_fetch(
    publisher: &dyn EventPublisher,
    url: &str,
    wallpaper_dir: &str,
    id: &str,
) {
    match wallhaven::download(url, wallpaper_dir, id) {
        Ok(path) => {
            let resolved = await_converted(wallpaper_dir, id, &path, 1500);
            publisher.publish(
                ev::DOWNLOAD,
                wall_proto::DownloadEvent {
                    path: Some(resolved),
                    ..wall_proto::DownloadEvent::new(id, wall_proto::dl_status::DONE)
                }
                .to_value(),
            );
        }
        Err(error) => publisher.publish(
            ev::DOWNLOAD,
            wall_proto::DownloadEvent {
                error: Some(error.to_string()),
                ..wall_proto::DownloadEvent::new(id, wall_proto::dl_status::ERROR)
            }
            .to_value(),
        ),
    }
}
