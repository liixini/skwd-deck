#[allow(clippy::wildcard_imports)]
use super::common::*;

use crate::infrastructure::events::EventHub;

use super::source_steam::{steam_download, steam_preview, steam_search};
use super::source_wallhaven::{wallhaven_download, wallhaven_preview, wallhaven_search};
use super::source_youtube::youtube_source_download;

mod lifecycle;
#[cfg(test)]
use lifecycle::{LIST_CANCELLED, LIST_TIMEOUT, ListCall};
pub(super) use lifecycle::{LIST_DEADLINE, ListLifecycle, list_call, run_list};

pub(super) fn handle_remote_preview<F>(
    req_id: u64,
    id: &str,
    url: String,
    dest: std::path::PathBuf,
    events: &Arc<EventHub>,
    policy: F,
) -> Response
where
    F: Fn(&str) -> anyhow::Result<()> + Send + 'static,
{
    let key = format!("preview:{}", dest.to_string_lossy());
    if !dl_inflight_begin(&key) {
        return Response::ok(req_id, json!({"id": id, "status": "fetching"}));
    }
    let Some(reservation) = crate::infrastructure::dlqueue::PREVIEWS.try_reserve() else {
        dl_inflight_end(&key);
        return Response::err(req_id, -32000, "preview queue is full; retry shortly");
    };
    let events = Arc::clone(events);
    let id2 = id.to_string();
    let guard = InflightGuard(key, dl_inflight_end);
    match thread::Builder::new().name("skwd-preview".into()).spawn(move || {
        let _guard = guard;
        let _slot = reservation.acquire(|_| {});
        if dest.exists() {
            match crate::infrastructure::http::validate_cached_preview(&dest) {
                Ok(()) => {
                    events.publish(
                        ev::PREVIEW_READY,
                        json!({"id": id2, "path": dest.to_string_lossy()}),
                    );
                    return;
                }
                Err(error) => {
                    log::warn!("discarding unsafe cached preview {}: {error}", dest.display());
                    let _ = std::fs::remove_file(&dest);
                }
            }
        }
        match crate::infrastructure::http::fetch_image(&url, &dest, policy) {
            Ok(()) => events
                .publish(ev::PREVIEW_READY, json!({"id": id2, "path": dest.to_string_lossy()})),
            Err(err) => log::warn!("preview fetch {id2} failed: {err}"),
        }
    }) {
        Ok(_) => Response::ok(req_id, json!({"id": id, "status": "fetching"})),
        Err(error) => Response::err(req_id, -1, format!("failed to start preview fetch: {error}")),
    }
}

pub(super) fn require_enabled(
    state: &Arc<WallState>,
    req: &Request,
    stats: &Arc<Stats>,
    source: &str,
) -> Option<Response> {
    state.reload_config();
    let (on, msg) = if source == "steam" {
        (state.config().steam_enabled(), "steam/WE feature is disabled".to_string())
    } else {
        (state.config().source_enabled(source), format!("{source} source is disabled"))
    };
    if on {
        return None;
    }
    stats.error();
    Some(Response::err(req.id, -1, msg))
}

pub(super) fn require_id_url(
    req: &Request,
    stats: &Arc<Stats>,
    count_err: bool,
) -> Result<(String, String), Response> {
    let id = req.str_param("id", "").to_string();
    let url = req.str_param("full_url", "").to_string();
    if id.is_empty() || url.is_empty() {
        if count_err {
            stats.error();
        }
        return Err(Response::err(req.id, -32602, "missing id/full_url"));
    }
    Ok((id, url))
}

pub(super) fn respond_exists(
    req_id: u64,
    publisher: &dyn EventPublisher,
    id: &str,
    existing: &std::path::Path,
) -> Response {
    let path = existing.to_string_lossy().to_string();
    let payload = wall_proto::DownloadEvent {
        path: Some(path.clone()),
        ..wall_proto::DownloadEvent::new(id, wall_proto::dl_status::DONE)
    };
    publisher.publish(ev::DOWNLOAD, payload.to_value());
    Response::ok(req_id, json!({"id": id, "status": "exists", "path": path}))
}

pub(super) fn generic_source_list(
    ctx: &Ctx,
    req: &Request,
    spec: &wall_proto::sources::SourceSpec,
) -> Response {
    let Ctx { state, workers, stats, .. } = ctx;
    let source = spec.key;
    if let Some(resp) = require_enabled(state, req, stats, source) {
        return resp;
    }
    let (bing_market, unsplash_key, pexels_key, wallpaper_dir, video_dir) = {
        let cfg = state.config();
        (
            cfg.bing_market(),
            cfg.unsplash_access_key(),
            cfg.pexels_api_key(),
            cfg.wallpaper_dir(),
            cfg.video_dir(),
        )
    };
    let page = req.opt_i64("page").unwrap_or(1).max(1) as u32;
    let query = req.str_param("query", "").to_string();
    let result = match source {
        "bing" => sources::bing::search(&bing_market),
        "unsplash" => sources::unsplash::search(
            &unsplash_key,
            &query,
            page,
            30,
            req.str_param("order_by", "relevant"),
            req.str_param("orientation", ""),
            req.str_param("color", ""),
            req.str_param("content_filter", "high"),
        ),
        "pexels" => sources::pexels::search(
            &pexels_key,
            &query,
            page,
            30,
            req.str_param("orientation", ""),
            req.str_param("size", ""),
            req.str_param("color", ""),
        ),
        "youtube" => {
            let max_dur = req.opt_i64("max_duration").unwrap_or(0).max(0) as u64;
            sources::youtube::search(&query, page, 24, max_dur)
        }
        _ => return Response::err(req.id, -32602, format!("unknown source '{source}'")),
    };
    match result {
        Ok(search_page) => {
            let lib_dir = if spec.media == wall_proto::sources::Media::Video {
                &video_dir
            } else {
                &wallpaper_dir
            };
            let local = sources::library_ids(lib_dir, source);
            let items: Vec<wall_proto::sources::ListItem> = search_page
                .results
                .iter()
                .map(|res| {
                    let thumb_path = skwd_wall_core::paths::remote_thumb(source, &res.id);
                    let (attr, attr_url) = res
                        .attribution
                        .as_ref()
                        .map(|attr| (attr.text.clone(), attr.link.clone()))
                        .unwrap_or_default();
                    wall_proto::sources::ListItem {
                        id: res.id.clone(),
                        full_url: res.full_url.clone(),
                        thumb_url: res.thumb_url.clone(),
                        thumb_path: thumb_path.to_string_lossy().into_owned(),
                        resolution: res.resolution.clone(),
                        file_size: res.file_size,
                        title: res.title.clone(),
                        attribution: attr,
                        attribution_url: attr_url,
                        track_url: res.track_url.clone(),
                        duration_secs: res.duration_secs,
                        downloaded: local.contains(&res.id),
                        ..wall_proto::sources::ListItem::default()
                    }
                })
                .collect();
            let jobs: Vec<(String, String)> = search_page
                .results
                .iter()
                .filter(|res| !res.thumb_url.is_empty())
                .map(|res| (res.id.clone(), res.thumb_url.clone()))
                .collect();
            workers.remote_thumbnails(source, &jobs);
            Response::ok(
                req.id,
                json!(wall_proto::sources::ListResult {
                    generation: req.opt_u64("generation"),
                    results: items,
                    last_page: search_page.last_page,
                    current_page: search_page.current_page,
                    next_cursor: search_page.next_cursor,
                }),
            )
        }
        Err(err) => fail(stats, req.id, err),
    }
}

pub(super) fn generic_source_preview(
    req: &Request,
    events: &Arc<EventHub>,
    stats: &Arc<Stats>,
    spec: &wall_proto::sources::SourceSpec,
) -> Response {
    let source = spec.key;
    let (id, mut url) = match require_id_url(req, stats, false) {
        Ok(pair) => pair,
        Err(resp) => return resp,
    };
    if source == "youtube" {
        if !sources::youtube::safe_id(&id) {
            return crate::infrastructure::rpc::fail_msg(
                stats,
                req.id,
                -32602,
                "invalid youtube id",
            );
        }
        url = sources::youtube::preview_for(&id);
    }
    if crate::infrastructure::http::require_source(source, &url).is_err() {
        return crate::infrastructure::rpc::fail_msg(stats, req.id, -1, "blocked preview url");
    }
    let dest = skwd_wall_core::paths::remote_preview(source, &id, sources::ext_from_url(&url));
    let src = source.to_string();
    handle_remote_preview(req.id, &id, url, dest, events, move |url| {
        crate::infrastructure::http::require_source(&src, url)
    })
}

static DL_INFLIGHT: std::sync::LazyLock<std::sync::Mutex<std::collections::HashSet<String>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashSet::new()));

pub(super) fn dl_inflight_begin(key: &str) -> bool {
    DL_INFLIGHT.lock().is_ok_and(|mut set| set.insert(key.to_string()))
}

pub(super) fn dl_inflight_end(key: &str) {
    if let Ok(mut set) = DL_INFLIGHT.lock() {
        set.remove(key);
    }
}

pub(super) struct InflightGuard(pub String, pub fn(&str));

impl Drop for InflightGuard {
    fn drop(&mut self) {
        (self.1)(&self.0);
    }
}

pub(super) fn spawn_download<F>(
    req_id: u64,
    id: &str,
    events: &Arc<EventHub>,
    stats: &Arc<Stats>,
    queue: &'static crate::infrastructure::dlqueue::Gate,
    inflight_key: String,
    work: F,
) -> Response
where
    F: FnOnce(&Arc<EventHub>, &Arc<Stats>, crate::infrastructure::dlqueue::Slot<'static>)
        + Send
        + 'static,
{
    if !dl_inflight_begin(&inflight_key) {
        return Response::ok(req_id, json!({"id": id, "status": "in_progress"}));
    }
    let guard = InflightGuard(inflight_key, dl_inflight_end);
    let Some(reservation) = queue.try_reserve() else {
        drop(guard);
        return Response::err(req_id, -32000, "download queue is full; retry shortly");
    };
    let events = Arc::clone(events);
    let stats2 = Arc::clone(stats);
    let id2 = id.to_string();
    match thread::Builder::new().name("skwd-download".into()).spawn(move || {
        let _guard = guard;
        let slot = reservation.acquire(|ahead| {
            events.publish(
                ev::DOWNLOAD,
                wall_proto::DownloadEvent {
                    progress: Some(0.0),
                    message: Some(crate::infrastructure::dlqueue::queue_label(ahead)),
                    ..wall_proto::DownloadEvent::new(&id2, wall_proto::dl_status::QUEUED)
                }
                .to_value(),
            );
        });
        work(&events, &stats2, slot);
    }) {
        Ok(_) => Response::ok(req_id, json!({"id": id, "status": "started"})),
        Err(error) => {
            stats.error();
            Response::err(req_id, -1, format!("failed to start download: {error}"))
        }
    }
}

pub(super) fn generic_source_download(ctx: &Ctx, req: &Request, source: &str) -> Response {
    let Ctx { state, events, workers, stats, .. } = ctx;
    if let Some(resp) = require_enabled(state, req, stats, source) {
        return resp;
    }
    let (id, url) = match require_id_url(req, stats, true) {
        Ok(pair) => pair,
        Err(resp) => return resp,
    };
    let track_url = req.str_param("track_url", "").to_string();
    let access_key = state.config().unsplash_access_key();
    let wdir = state.config().wallpaper_dir();
    if let Some(existing) = sources::library_path(&wdir, source, &id) {
        return respond_exists(req.id, events.as_ref(), &id, &existing);
    }
    let workers = Arc::clone(workers);
    let id2 = id.clone();
    let source2 = source.to_string();
    spawn_download(
        req.id,
        &id,
        events,
        stats,
        &crate::infrastructure::dlqueue::IMAGES,
        format!("{source}:{id}"),
        move |events, _stats, slot| {
            events.publish(
                ev::DOWNLOAD,
                wall_proto::DownloadEvent {
                    progress: Some(0.0),
                    message: Some("fetching".to_string()),
                    ..wall_proto::DownloadEvent::new(&id2, wall_proto::dl_status::DOWNLOADING)
                }
                .to_value(),
            );
            if source2 == "unsplash" {
                let _ = sources::unsplash::track_download(&track_url, &access_key);
            }
            let mut on_progress = |pct: f64| {
                events.publish(
                    ev::DOWNLOAD,
                    wall_proto::DownloadEvent {
                        progress: Some(pct),
                        message: Some("fetching".to_string()),
                        ..wall_proto::DownloadEvent::new(&id2, wall_proto::dl_status::DOWNLOADING)
                    }
                    .to_value(),
                );
            };
            let fetched =
                sources::download_with_progress(&source2, &url, &wdir, &id2, &mut on_progress);
            drop(slot);
            match fetched {
                Ok(path) => {
                    workers.scan(&[], None);
                    let resolved = await_converted_by(&path, 1500, || {
                        sources::library_path(&wdir, &source2, &id2)
                    });
                    events.publish(
                        ev::DOWNLOAD,
                        wall_proto::DownloadEvent {
                            path: Some(resolved),
                            ..wall_proto::DownloadEvent::new(&id2, wall_proto::dl_status::DONE)
                        }
                        .to_value(),
                    );
                }
                Err(err) => events.publish(
                    ev::DOWNLOAD,
                    wall_proto::DownloadEvent {
                        error: Some(err.to_string()),
                        ..wall_proto::DownloadEvent::new(&id2, wall_proto::dl_status::ERROR)
                    }
                    .to_value(),
                ),
            }
        },
    )
}

pub(super) fn source_rpc(ctx: &Ctx, req: &Request) -> Response {
    let Ctx { events, stats, .. } = ctx;
    let source = req.str_param("source", "");
    let Some(spec) = wall_proto::sources::spec(source) else {
        return Response::err(req.id, -32602, format!("unknown source '{source}'"));
    };
    let verb = req.method.trim_start_matches("source.");
    match spec.provider {
        wall_proto::sources::Provider::Wallhaven => match verb {
            "list" => wallhaven_search(ctx, req),
            "preview" => wallhaven_preview(req, events, stats),
            "download" => wallhaven_download(ctx, req),
            _ => Response::err(req.id, -32601, format!("unknown source verb '{verb}'")),
        },
        wall_proto::sources::Provider::Steam => match verb {
            "list" => steam_search(ctx, req),
            "preview" => steam_preview(ctx, req),
            "download" => steam_download(ctx, req),
            _ => Response::err(req.id, -32601, format!("unknown source verb '{verb}'")),
        },
        wall_proto::sources::Provider::Unsplash
        | wall_proto::sources::Provider::Pexels
        | wall_proto::sources::Provider::Youtube
        | wall_proto::sources::Provider::Bing => match verb {
            "list" => generic_source_list(ctx, req, spec),
            "preview" => generic_source_preview(req, events, stats, spec),
            "download" if spec.media == wall_proto::sources::Media::Video => {
                youtube_source_download(ctx, req)
            }
            "download" => generic_source_download(ctx, req, source),
            _ => Response::err(req.id, -32601, format!("unknown source verb '{verb}'")),
        },
    }
}

#[cfg(test)]
#[path = "source/tests.rs"]
mod tests;
