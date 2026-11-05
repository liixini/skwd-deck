use std::sync::Arc;
use std::thread;

use serde_json::json;
use wall_proto::sources::{PreviewResponse, PreviewStatus};
use wall_proto::{Request, Response, ev};

use super::{InflightGuard, dl_inflight_begin, dl_inflight_end, preview_cache};
use crate::backend::events::EventPublisher;
use crate::infrastructure::events::EventHub;

const REFRESH_PARAM: &str = "refresh";
const QUEUE_FULL_CODE: i32 = -32000;
const QUEUE_FULL_MESSAGE: &str = "preview queue is full; retry shortly";

fn publish_preview_result(
    events: &EventHub,
    id: &str,
    source: &str,
    url: &str,
    path: &str,
    error: &str,
) {
    events.publish(
        ev::PREVIEW_READY,
        json!(ev::PreviewReady {
            id: id.to_string(),
            path: path.to_string(),
            source: source.to_string(),
            url: url.to_string(),
            error: error.to_string(),
        }),
    );
}

pub(in crate::infrastructure::rpc) fn handle_remote_preview<F>(
    request: &Request,
    source: &str,
    id: &str,
    url: String,
    dest: std::path::PathBuf,
    events: &Arc<EventHub>,
    policy: F,
) -> Response
where
    F: Fn(&str) -> anyhow::Result<()> + Send + 'static,
{
    let refresh = request.bool_param(REFRESH_PARAM, false);
    if dest.exists() && !refresh {
        match crate::infrastructure::http::validate_cached_preview_header(&dest) {
            Ok(()) => {
                let path = dest.to_string_lossy().into_owned();
                publish_preview_result(events, id, source, &url, &path, "");
                return Response::ok(
                    request.id,
                    json!(PreviewResponse {
                        id: id.to_string(),
                        status: PreviewStatus::Ready,
                        path: Some(path),
                    }),
                );
            }
            Err(error) => {
                log::warn!("discarding unsafe cached preview {}: {error}", dest.display());
                let _ = std::fs::remove_file(&dest);
            }
        }
    }
    let key = format!("preview:{}", dest.to_string_lossy());
    if !dl_inflight_begin(&key) {
        return Response::ok(
            request.id,
            json!(PreviewResponse {
                id: id.to_string(),
                status: PreviewStatus::Fetching,
                path: None,
            }),
        );
    }
    let Some(reservation) = crate::infrastructure::dlqueue::PREVIEWS.try_reserve() else {
        dl_inflight_end(&key);
        return Response::err(request.id, QUEUE_FULL_CODE, QUEUE_FULL_MESSAGE);
    };
    let events = Arc::clone(events);
    let id2 = id.to_string();
    let source = source.to_string();
    let guard = InflightGuard(key, dl_inflight_end);
    match thread::Builder::new().name("skwd-preview".into()).spawn(move || {
        let _guard = guard;
        let _slot = reservation.acquire(|_| {});
        if refresh {
            let _ = std::fs::remove_file(&dest);
        }
        match crate::infrastructure::http::fetch_image(&url, &dest, policy) {
            Ok(()) => {
                publish_preview_result(&events, &id2, &source, &url, &dest.to_string_lossy(), "");
                if let Err(error) = preview_cache::prune(&dest) {
                    log::warn!("preview cache prune failed: {error}");
                }
            }
            Err(err) => {
                log::warn!("preview fetch {id2} failed: {err}");
                publish_preview_result(&events, &id2, &source, &url, "", &err.to_string());
            }
        }
    }) {
        Ok(_) => Response::ok(
            request.id,
            json!(PreviewResponse {
                id: id.to_string(),
                status: PreviewStatus::Fetching,
                path: None,
            }),
        ),
        Err(error) => {
            Response::err(request.id, -1, format!("failed to start preview fetch: {error}"))
        }
    }
}
