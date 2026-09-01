use serde_json::{Value, json};
use skwd_wall_core::db;
use wall_proto::{Request, Response, ev, rpc};

use crate::backend::events::EventPublisher;
use crate::composition::context::Ctx;
use crate::composition::history::history_nav;
use crate::composition::runtime::{playlist, schedule};
use crate::infrastructure::effects_preview::{effect_ids, effects_list, safe_remove_preview};
use crate::infrastructure::{doctor, overview_backdrop, workspaces};

#[allow(clippy::wildcard_imports)]
use super::handlers::*;
use super::response::fail;
#[allow(clippy::wildcard_imports)]
use super::source::*;
#[allow(clippy::wildcard_imports)]
use super::source_steam::*;
#[allow(clippy::wildcard_imports)]
use super::source_wallhaven::*;
use super::tags::update_tags;
#[allow(clippy::wildcard_imports)]
use super::wallpaper::*;

#[cfg(test)]
use super::connection::{handle_conn, peer_uid_allowed};
#[cfg(test)]
use super::response::classify_apply_error;

fn relay(publisher: &dyn EventPublisher, req: &Request, event: &str) -> Response {
    publisher.publish(event, req.params.clone());
    Response::ok(req.id, json!({"ok": true}))
}

fn task_control(ctx: &Ctx, req: &Request) -> Response {
    let id = req.str_param("id", "");
    let action = req.str_param("action", "");
    let semantic = || {
        let control = match action {
            "pause" => wall_proto::TaskControl::Pause,
            "resume" => wall_proto::TaskControl::Resume,
            "stop" => wall_proto::TaskControl::Stop,
            _ => return false,
        };
        crate::infrastructure::semantic_index::control(control)
    };
    match (id, action) {
        (id, "stop") if id.starts_with("tinier:") && ctx.workers.stop_tinier(id) => {
            Response::ok(req.id, json!({"ok": true}))
        }
        ("semantic-index", "pause" | "resume" | "stop") if semantic() => {
            Response::ok(req.id, json!({"ok": true}))
        }
        ("", _) | (_, "") => Response::err(req.id, -32602, "missing task id or action"),
        _ => Response::err(req.id, -32601, "this task does not support that control"),
    }
}

pub(crate) fn dispatch(ctx: &Ctx, req: &Request) -> Response {
    let Ctx { state, events, workers, stats, .. } = ctx;
    stats.rpc(&req.method);
    log::debug!("rpc {} id={}", req.method, req.id);
    ctx.renderers.reap_exited();
    if req.method == rpc::PAPER_READY {
        signal_ready(ctx.renderers.as_ref(), req);
        return Response::ok(req.id, json!({"ok": true}));
    }
    match req.method.as_str() {
        rpc::STATUS => Response::ok(req.id, runtime_status(ctx)),
        rpc::PICKER_SESSION_BEGIN => Response::ok(req.id, json!({"ok": true, "visible": true})),
        rpc::PICKER_SESSION_END => Response::ok(req.id, json!({"ok": true, "visible": false})),
        rpc::WALL_PREHEAT => preheat(req),
        rpc::WALL_SET_PAUSED => wall_set_paused(state, ctx.renderers.as_ref(), req),
        rpc::WALL_PLAYLIST_NEXT | rpc::WALL_PLAYLIST_PREV => {
            let output = req.str_param("output", "*");
            let forward = req.method.ends_with("next");
            let ok = playlist::command(state, output, forward);
            Response::ok(req.id, json!({"ok": ok, "output": output, "forward": forward}))
        }
        rpc::WALL_PLAYLIST_RELOAD => {
            playlist::reload();
            Response::ok(req.id, json!({"ok": true}))
        }
        rpc::PLAYLIST_LIST => playlist_list(state, req),
        rpc::PLAYLIST_CREATE => playlist_create(state, req, stats),
        rpc::PLAYLIST_DELETE => {
            let id = req.opt_i64("id").unwrap_or(0);
            let _ = state.with_db(|conn| db::playlist_delete(conn, id));
            playlist::reload();
            Response::ok(req.id, json!({"ok": true}))
        }
        rpc::PLAYLIST_UPDATE => playlist_update(state, req),
        rpc::PLAYLIST_MEMBERS => {
            let id = req.opt_i64("id").unwrap_or(0);
            let members = playlist::resolve_member_items(state, id);
            Response::ok(req.id, json!({"id": id, "members": members}))
        }
        rpc::PLAYLIST_ADD | rpc::PLAYLIST_REMOVE | rpc::PLAYLIST_TOGGLE => {
            playlist_member_op(state, req)
        }
        rpc::PLAYLIST_MOVE => playlist_move(state, req),
        rpc::PLAYLIST_ASSIGN => {
            let output = req.str_param("output", "*");
            let id = req.opt_i64("id").filter(|&id| id > 0);
            let _ = state.with_db(|conn| db::playlist_assign_set(conn, output, id));
            playlist::reload();
            Response::ok(req.id, json!({"ok": true}))
        }
        rpc::PLAYLIST_STOP => {
            let id = req.opt_i64("id").filter(|&id| id > 0);
            let cleared = state.with_db(|conn| db::playlist_assign_clear(conn, id)).unwrap_or(0);
            playlist::reload();
            Response::ok(req.id, json!({"ok": true, "cleared": cleared}))
        }
        rpc::PLAYLIST_MEMBERSHIPS => {
            let key = req.str_param("key", "");
            let ids = state
                .with_db(|conn| db::playlist_memberships_for_key(conn, key))
                .unwrap_or_default();
            Response::ok(req.id, json!({"key": key, "ids": ids}))
        }
        rpc::SCHEDULE_RELOAD => {
            schedule::reload();
            Response::ok(req.id, json!({"ok": true}))
        }
        rpc::WORKSPACE_RELOAD => {
            workspaces::reload(state);
            Response::ok(req.id, json!({"ok": true}))
        }
        rpc::WORKSPACE_LIST => workspace_list(state, req),
        rpc::DIAG => diag(ctx, req),
        rpc::STATUS_DOCTOR => Response::ok(req.id, doctor::checks_json()),
        rpc::STATUS_BUG_REPORT => match doctor::bug_report_to_file() {
            Ok(path) => Response::ok(req.id, json!({"path": path.display().to_string()})),
            Err(err) => fail(&ctx.stats, req.id, err),
        },
        rpc::WALL_WEATHER => {
            let (locale, lat, lon) = {
                let cfg = ctx.config.read();
                (cfg.locale(), cfg.latitude(), cfg.longitude())
            };
            Response::ok(
                req.id,
                json!({"weather": crate::infrastructure::weather::current(&locale, lat, lon)}),
            )
        }
        rpc::SUBSCRIBE => Response::ok(req.id, json!({"subscribed": true})),
        rpc::SCAN_ITEM => {
            stats.thumb();
            events.publish(ev::CACHED, req.params.clone());
            Response::ok(req.id, json!({"ok": true}))
        }
        rpc::SCAN_DONE => scan_done(ctx, req),
        rpc::SCAN_REMOVED => relay(events.as_ref(), req, ev::REMOVED),
        rpc::REMOTE_THUMB => relay(events.as_ref(), req, ev::REMOTE_THUMB),
        rpc::RECOMPUTE_PROGRESS => relay(events.as_ref(), req, ev::RECOMPUTE_PROGRESS),
        rpc::RECOMPUTE_DONE => {
            stats.set_task("idle");
            events.publish(ev::RECOMPUTE_COMPLETE, req.params.clone());
            Response::ok(req.id, json!({"ok": true}))
        }
        rpc::EFFECTS_LIST => match effects_list() {
            Ok(list) => Response::ok(req.id, json!({"effects": list})),
            Err(err) => Response::err(req.id, -32603, format!("effects.list: {err}")),
        },
        rpc::EFFECTS_PREVIEW => effects_preview_rpc(req),
        rpc::EFFECTS_COMMIT => effects_commit_rpc(state, req),
        rpc::EFFECTS_DISCARD => {
            if let Some(preview) = req.opt_str("preview") {
                safe_remove_preview(preview);
            }
            Response::ok(req.id, json!({"ok": true}))
        }
        rpc::EFFECTS_BACKFILL_TAGS => {
            let ids = effect_ids();
            let tagged = state.with_db(|conn| db::backfill_effect_tags(conn, &ids)).unwrap_or(0);
            log::info!("effects.backfill_tags: tagged {tagged} existing effect wallpaper(s)");
            Response::ok(req.id, json!({"tagged": tagged}))
        }
        rpc::WALL_OUTPUTS => outputs_list(state, ctx.renderers.as_ref(), req),
        rpc::WALL_RETHEME => wall_retheme(state, req, stats),
        rpc::THEME_BACKENDS => {
            state.reload_config();
            let cfg = state.config().clone();
            let available = skwd_wall_core::theme::available_backends(&cfg);
            Response::ok(req.id, json!({ "backends": available }))
        }
        rpc::THEME_PREVIEW => theme_preview(state, req),
        rpc::THEME_PREVIEWS => theme_previews(state, req),
        rpc::WALL_CLEAR_DATA => clear_data(ctx, req),
        rpc::WALL_RECOMPUTE_COLORS => {
            stats.set_task("recomputing colors");
            workers.scan(&["--recolor"], None);
            Response::ok(req.id, json!({"started": true}))
        }
        rpc::WALL_REFRESH_OVERVIEW_BACKDROP => {
            state.reload_config();
            let cfg = state.config().clone();
            overview_backdrop::refresh_from_disk(&cfg);
            Response::ok(req.id, json!({"ok": true}))
        }
        rpc::WALL_REMOVE => crate::infrastructure::removal::handle_wall_remove(ctx, req),
        rpc::TASK_LIST => Response::ok(req.id, json!({"tasks": ctx.tasks.list()})),
        rpc::TASK_CONTROL => task_control(ctx, req),
        rpc::OPTIMIZE_START => {
            state.reload_config();
            let started = workers.optimize_images(false, &[]);
            Response::ok(req.id, json!({"started": started}))
        }
        rpc::OPTIMIZE_STATUS => Response::ok(req.id, workers.image_optimization_status()),
        rpc::SOURCE_LIST | rpc::SOURCE_PREVIEW | rpc::SOURCE_DOWNLOAD => source_rpc(ctx, req),
        rpc::WALLHAVEN_SEARCH => wallhaven_search(ctx, req),
        rpc::WALLHAVEN_COLLECTIONS => wallhaven_collections(state, req, stats),
        rpc::WALLHAVEN_PREVIEW => wallhaven_preview(req, events, stats),
        rpc::WALLHAVEN_DOWNLOAD => wallhaven_download(ctx, req),
        rpc::STEAM_SEARCH => steam_search(ctx, req),
        rpc::STEAM_PREVIEW => steam_preview(ctx, req),
        rpc::STEAM_DOWNLOAD => steam_download(ctx, req),
        rpc::WALL_APPLY => wall_apply(ctx, req),
        rpc::WALL_WE_PROPERTIES => wall_we_properties(ctx, req),
        rpc::WALL_SET_WE_PROPERTY => wall_set_we_property(ctx, req),
        rpc::WALL_HISTORY_BACK | rpc::WALL_HISTORY_FORWARD => {
            let output = req.str_param("output", "*").to_string();
            let forward = req.method == rpc::WALL_HISTORY_FORWARD;
            Response::ok(
                req.id,
                history_nav(
                    state,
                    ctx.wallpaper.as_ref(),
                    ctx.history.as_ref(),
                    events.as_ref(),
                    stats,
                    &output,
                    forward,
                ),
            )
        }
        rpc::WALL_HISTORY_LIST => {
            let output = req.str_param("output", "*");
            let outputs: serde_json::Map<String, Value> = ctx
                .history
                .list(output)
                .into_iter()
                .map(|(name, history)| {
                    (
                        name,
                        json!({
                            "entries": history.entries,
                            "pos": history.pos,
                        }),
                    )
                })
                .collect();
            Response::ok(req.id, json!({ "outputs": outputs }))
        }
        rpc::WALL_SET_AUDIO => {
            set_audio(state, ctx.renderers.as_ref(), ctx.wallpaper.as_ref(), req)
        }
        rpc::WALL_MONITORS => monitors(req),
        rpc::WALL_FORGET_MONITOR => forget_monitor(state, req),
        rpc::WALL_ROTATION_WAKE => {
            crate::composition::runtime::rotation::wake();
            Response::ok(req.id, json!({"ok": true}))
        }
        rpc::WALL_SHELL_PREVIEW => shell_preview(state, req, stats),
        rpc::WALL_SHELL_PREVIEW_END => shell_preview_end(state, req),
        rpc::WALL_RELOAD_WE => wall_reload_we(ctx, req),
        rpc::WALL_SET_FAVOURITE => set_favourite(state, req, stats),
        rpc::WALL_UPDATE_TAGS | rpc::WALL_UPDATE_ANALYSIS => update_tags(state, req, stats),
        other => {
            stats.error();
            Response::err(req.id, -32601, format!("unknown method '{other}'"))
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
