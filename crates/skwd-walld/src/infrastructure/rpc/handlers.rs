use std::sync::Arc;

use serde_json::{Value, json};
use skwd_wall_core::{WallState, db};
use wall_proto::{Request, Response, ev};

use super::response::fail;
use crate::backend::events::EventPublisher;
use crate::composition::bootstrap::vram_mb;
use crate::composition::context::Ctx;
use crate::composition::runtime::playlist;
use crate::infrastructure::effects_preview::{effects_preview, requested_effects};
use crate::infrastructure::stats::Stats;
use crate::infrastructure::workspaces;

pub(super) fn status_payload(
    result: Result<skwd_wall_core::infrastructure::paper::CapabilitiesResult, String>,
) -> Value {
    let mut status = wall_proto::deck_status(skwd_wall_core::version());
    match result {
        Ok(capabilities) if !capabilities.renderers.is_empty() => {
            status["renderers"] = json!(capabilities.renderers);
        }
        Ok(_) => {
            status["renderers"] = json!([]);
            status["renderer_error"] =
                json!("Paper did not report runtime renderer capabilities; upgrade skwd-paper");
        }
        Err(error) => {
            status["renderers"] = json!([]);
            status["renderer_error"] = json!(error);
        }
    }
    status
}

pub(super) fn runtime_status(ctx: &Ctx) -> Value {
    let config = ctx.config.read().clone();
    let result = skwd_wall_core::infrastructure::paper::PaperClient::configured(&config)
        .capabilities()
        .map_err(|error| format!("runtime renderer capabilities are unavailable: {error:#}"));
    let mut status = status_payload(result);
    status["library_watch"] = json!(crate::infrastructure::watcher::current_status());
    status
}

pub(super) fn wall_set_paused(
    state: &WallState,
    renderers: &dyn skwd_wall_core::backend::renderers::RendererSupervision,
    request: &Request,
) -> Response {
    let _apply = state.apply().lock();
    let paused = request.params.get("paused").and_then(serde_json::Value::as_bool).unwrap_or(false);
    renderers.set_paused(paused);
    if skwd_wall_core::plasma::available()
        && let Err(error) = skwd_wall_core::plasma::apply_current(state)
    {
        log::warn!("set_paused: Plasma update failed: {error:#}");
    }
    log::info!("set_paused: {paused}");
    Response::ok(request.id, json!({"ok": true, "paused": paused}))
}

pub(super) fn playlist_list(state: &Arc<WallState>, request: &Request) -> Response {
    let playlists = playlist::list_with_resolved_counts(state);
    let assignments: Vec<wall_proto::PlaylistAssign> = state
        .with_db(db::playlist_assigns)
        .unwrap_or_default()
        .into_iter()
        .map(|(output, id)| wall_proto::PlaylistAssign { output, id })
        .collect();
    Response::ok(request.id, json!({"playlists": playlists, "assign": assignments}))
}

pub(super) fn playlist_create(
    state: &Arc<WallState>,
    request: &Request,
    stats: &Arc<Stats>,
) -> Response {
    let name = request.str_param("name", "New playlist");
    match state.with_db(|connection| db::playlist_create(connection, name)) {
        Ok(id) => {
            playlist::reload();
            Response::ok(request.id, json!({"id": id}))
        }
        Err(error) => fail(stats, request.id, error),
    }
}

pub(super) fn playlist_update(state: &Arc<WallState>, request: &Request) -> Response {
    let id = request.opt_i64("id").unwrap_or(0);
    let name = request.opt_str("name");
    let kind = request.opt_str("kind");
    let source = request.opt_str("source");
    let order = request.opt_str("order");
    let dwell = request.opt_i64("dwell");
    let _ = state.with_db(|connection| {
        db::playlist_update(connection, id, name, kind, source, order, dwell)
    });
    playlist::reload();
    Response::ok(request.id, json!({"ok": true}))
}

pub(super) fn playlist_member_op(state: &Arc<WallState>, request: &Request) -> Response {
    let id = request.opt_i64("id").unwrap_or(0);
    let key = request.str_param("key", "");
    let included = match request.method.rsplit('.').next() {
        Some("add") => {
            let _ = state.with_db(|connection| db::playlist_add_member(connection, id, key));
            true
        }
        Some("remove") => {
            let _ = state.with_db(|connection| db::playlist_remove_member(connection, id, key));
            false
        }
        _ => state
            .with_db(|connection| db::playlist_toggle_member(connection, id, key))
            .unwrap_or(false),
    };
    playlist::reload();
    Response::ok(request.id, json!({"in": included, "id": id, "key": key}))
}

pub(super) fn playlist_move(state: &Arc<WallState>, request: &Request) -> Response {
    let id = request.opt_i64("id").unwrap_or(0);
    let key = request.str_param("key", "");
    let delta = request.opt_i64("delta").unwrap_or(0);
    let _ = state.with_db(|connection| db::playlist_move_member(connection, id, key, delta));
    playlist::reload();
    Response::ok(request.id, json!({"ok": true}))
}

pub(super) fn workspace_list(state: &Arc<WallState>, request: &Request) -> Response {
    let (enabled, rules) = {
        let config = state.config();
        (config.workspace_enabled(), config.workspace_wallpapers())
    };
    Response::ok(
        request.id,
        json!({ "workspaces": workspaces::list(), "enabled": enabled, "rules": rules }),
    )
}

pub(super) fn diag(ctx: &Ctx, request: &Request) -> Response {
    let Ctx { state, renderers, events, stats, .. } = ctx;
    Response::ok(
        request.id,
        json!({
            "banner": stats.banner(
                skwd_wall_core::diag::rss_mb(),
                renderers.wallpaper_rss_mb(),
                renderers.wallpaper_count(),
                renderers.scene_rss_mb(),
                renderers.scene_count(),
                state.scanner().scanner_rss_mb(),
                vram_mb(renderers.as_ref()),
                events.subscriber_count(),
            ),
            "counters": stats.counters_json(),
        }),
    )
}

pub(super) fn scan_done(ctx: &Ctx, request: &Request) -> Response {
    let Ctx { state, events, workers, stats, .. } = ctx;
    let count = request.params.get("count").and_then(serde_json::Value::as_i64).unwrap_or(0);
    let disk_full = request.bool_param("disk_full", false);
    let request_id = request.opt_str("request_id");
    stats.set_task("idle");
    let total = state.with_db(skwd_wall_core::db::item_count).unwrap_or(-1);
    log::info!("scan complete: {count} items (library total {total})");
    if let Some(request_id) = request_id {
        crate::infrastructure::watcher::complete_scan(events.as_ref(), request_id);
    }
    let mut completion = json!({"count": count, "total": total, "disk_full": disk_full});
    if let Some(request_id) = request_id {
        completion["request_id"] = json!(request_id);
    }
    events.publish(ev::SCAN_DONE, completion);
    ctx.tasks.finish("scan", wall_proto::TaskState::Completed, format!("{count} wallpapers found"));
    let changed_paths: Vec<String> = request
        .params
        .get("paths")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(String::from)
        .collect();
    workers.optimize_images(true, &changed_paths);
    crate::infrastructure::semantic_index::request_refresh();
    Response::ok(request.id, json!({"ok": true}))
}

pub(super) fn effects_preview_rpc(request: &Request) -> Response {
    let input = request.opt_str("input").unwrap_or_default().to_string();
    let effect = request.opt_str("effect").unwrap_or_default().to_string();
    let params = request.params.get("params").cloned().unwrap_or_else(|| json!({}));
    let effects = requested_effects(&effect, &params, request.params.get("effects"));
    match effects_preview(&input, &effects) {
        Ok(output) => Response::ok(request.id, json!({"output": output})),
        Err(error) => Response::err(request.id, -32603, format!("effects.preview: {error}")),
    }
}

pub(super) fn wall_retheme(
    state: &Arc<WallState>,
    request: &Request,
    stats: &Arc<Stats>,
) -> Response {
    state.reload_config();
    if let Some(source) = state.theme().source() {
        crate::infrastructure::theme_worker::theme_apply_async(&source);
        Response::ok(request.id, json!({"rethemed": true}))
    } else {
        stats.error();
        Response::err(request.id, 1, "no current wallpaper to retheme")
    }
}

pub(super) fn shell_preview_end(state: &Arc<WallState>, request: &Request) -> Response {
    state.theme().bump_shell_preview();
    let state = Arc::clone(state);
    std::thread::spawn(move || {
        for sink in &skwd_wall_core::theme_sink::SINKS {
            (sink.preview_end)(&state);
        }
    });
    Response::ok(request.id, json!({"ok": true}))
}

pub(super) fn monitors(request: &Request) -> Response {
    Response::ok(
        request.id,
        crate::infrastructure::restore_policy::known_monitors(&skwd_wall_core::outputs::enumerate()),
    )
}

pub(super) fn forget_monitor(state: &Arc<WallState>, request: &Request) -> Response {
    let id = request.params.get("id").and_then(Value::as_str).unwrap_or_default();
    if id.is_empty() {
        return Response::err(request.id, -1, String::from("forget_monitor needs an id"));
    }
    let cache = state.config().cache_dir();
    let forgotten = crate::infrastructure::restore_policy::forget_monitor(&cache, id);
    Response::ok(request.id, json!({ "forgotten": forgotten }))
}
