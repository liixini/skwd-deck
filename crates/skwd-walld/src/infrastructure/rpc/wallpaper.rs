#[allow(clippy::wildcard_imports)]
use super::common::*;

const PREHEAT_MAX_BYTES: usize = 32 << 20;

#[derive(Debug, PartialEq, Eq)]
enum DeferredTerminal {
    Success,
    Cancelled,
    Superseded,
    Failure(String),
}

fn preparation_terminal(
    generation: u64,
    current: u64,
    result: Result<(), String>,
) -> DeferredTerminal {
    if current != generation {
        return DeferredTerminal::Superseded;
    }
    match result {
        Ok(()) => DeferredTerminal::Success,
        Err(detail) if detail == "Preparation cancelled" => DeferredTerminal::Cancelled,
        Err(detail) => DeferredTerminal::Failure(detail),
    }
}

fn apply_terminal(result: anyhow::Result<serde_json::Value>) -> DeferredTerminal {
    match result {
        Ok(_) => DeferredTerminal::Success,
        Err(error)
            if error.downcast_ref::<crate::composition::apply::SupersededApply>().is_some() =>
        {
            DeferredTerminal::Superseded
        }
        Err(error) => DeferredTerminal::Failure(error.to_string()),
    }
}

pub(super) fn signal_ready(
    renderers: &dyn skwd_wall_core::backend::renderers::RendererSupervision,
    req: &Request,
) {
    if let Some(pid) = req.params.get("pid").and_then(serde_json::Value::as_u64) {
        renderers.signal_ready(pid as u32);
    }
}

pub(super) fn preheat(req: &Request) -> Response {
    let path = req.str_param("path", "").to_string();
    if !path.is_empty() {
        thread::spawn(move || preheat_read(&path));
    }
    Response::ok(req.id, json!({"ok": true}))
}

pub(super) fn preheat_read(path: &str) {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return;
    };
    let mut buf = vec![0u8; 1 << 20];
    let mut total = 0usize;
    while total < PREHEAT_MAX_BYTES {
        match file.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(got) => total += got,
        }
    }
}

pub(super) fn effects_commit_rpc(state: &Arc<WallState>, req: &Request) -> Response {
    let input = req.opt_str("input").unwrap_or_default().to_string();
    let effect = req.opt_str("effect").unwrap_or_default().to_string();
    let params = req.params.get("params").cloned().unwrap_or_else(|| json!({}));
    let effects = requested_effects(&effect, &params, req.params.get("effects"));
    let (wp_dir, vid_dir) = {
        let cfg = state.config();
        (cfg.wallpaper_dir(), cfg.video_dir())
    };
    let result = effects_commit(&input, &effects, &wp_dir, &vid_dir);
    if let Some(preview) = req.opt_str("preview") {
        safe_remove_preview(preview);
    }
    let out = match result {
        Ok(out) => out,
        Err(err) => return Response::err(req.id, -32603, format!("effects.commit: {err}")),
    };
    tag_effect_output(state, &wp_dir, &out, &effects);
    Response::ok(req.id, json!({"output": out}))
}

pub(super) fn tag_effect_output(state: &Arc<WallState>, wp_dir: &str, out: &str, effects: &Value) {
    let prefix = format!("{}/", wp_dir.trim_end_matches('/'));
    let Some(rel) = out.strip_prefix(&prefix) else {
        return;
    };
    let stem = rel.rsplit_once('.').map_or(rel, |(stem, _)| stem);
    let label = effect_chain_tag_label(effects);
    if label.is_empty() {
        return;
    }
    let _ = state.with_db(|conn| db::set_effect_tag(conn, stem, &label));
    log::info!("effects.commit: tagged {stem} with effect '{label}'");
}

pub(super) fn outputs_list(
    state: &Arc<WallState>,
    renderers: &dyn skwd_wall_core::backend::renderers::RendererSupervision,
    req: &Request,
) -> Response {
    let assigns = renderers.assignments();
    let (def_mute, def_vol) = {
        let cfg = state.config();
        (cfg.renderer().mute(), cfg.renderer().volume())
    };
    let cache_dir = state.config().cache_dir();
    let audio_state = skwd_wall_core::audio::read_state(&cache_dir);
    let shared_video = renderers.has_shared_video();
    let live = skwd_wall_core::outputs::enumerate();
    crate::infrastructure::restore_policy::remember_monitors(&live);
    let mut outputs: Vec<wall_proto::OutputStatus> = live
        .iter()
        .cloned()
        .map(|mon| {
            let (logical_width, logical_height) = mon.logical_size();
            let entry = audio_state.get(&mon.name).or_else(|| audio_state.get("*"));
            let getf = |key: &str| entry.and_then(|entry| entry.get(key));
            let raw_path = getf("path").and_then(serde_json::Value::as_str).unwrap_or("");
            let we_id = getf("we_id").and_then(serde_json::Value::as_str).unwrap_or("");
            let path_owned = library_path(state, raw_path);
            let path = path_owned.as_str();
            wall_proto::OutputStatus {
                target: mon.name.clone(),
                connected: true,
                current: current_of(path, we_id, &assigns, &mon.name),
                fill: state.config().display().fill_override_for(&mon.name).unwrap_or_default(),
                name: mon.name,
                width: mon.width,
                height: mon.height,
                logical_width,
                logical_height,
                kind: getf("type").and_then(serde_json::Value::as_str).unwrap_or("").to_string(),
                path: path.to_string(),
                we_id: we_id.to_string(),
                mute: getf("mute").and_then(serde_json::Value::as_bool).unwrap_or(def_mute),
                volume: getf("volume")
                    .and_then(serde_json::Value::as_u64)
                    .map_or(def_vol, |vol| vol as u32),
                audio_shared: shared_video,
            }
        })
        .collect();
    let remembered = crate::infrastructure::restore_policy::known_monitors(&live);
    if let Some(monitors) = remembered.get("monitors").and_then(Value::as_array) {
        for monitor in monitors
            .iter()
            .filter(|monitor| !monitor.get("connected").and_then(Value::as_bool).unwrap_or(false))
        {
            let field = |key| monitor.get(key).and_then(Value::as_str).unwrap_or_default();
            let wallpaper = monitor.get("wallpaper").and_then(Value::as_object);
            let wallpaper_field = |key| {
                wallpaper
                    .and_then(|entry| entry.get(key))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
            };
            let name = field("connector").to_string();
            let id = field("id");
            if name.is_empty() || id.is_empty() {
                continue;
            }
            let path_owned = library_path(state, wallpaper_field("path"));
            let kind = wallpaper_field("type").to_string();
            let we_id = wallpaper_field("we_id").to_string();
            let current =
                if kind == wall_proto::kind::WE { we_id.clone() } else { path_owned.clone() };
            outputs.push(wall_proto::OutputStatus {
                name: name.clone(),
                target: format!("@monitor:{id}"),
                connected: false,
                width: monitor.get("width").and_then(Value::as_i64).unwrap_or_default() as i32,
                height: monitor.get("height").and_then(Value::as_i64).unwrap_or_default() as i32,
                logical_width: 0,
                logical_height: 0,
                current,
                kind,
                path: path_owned,
                we_id,
                mute: def_mute,
                volume: def_vol,
                fill: state.config().display().fill_override_for(&name).unwrap_or_default(),
                audio_shared: false,
            });
        }
    }
    Response::ok(req.id, json!({"outputs": outputs}))
}

fn library_path(state: &Arc<WallState>, path: &str) -> String {
    if path.is_empty() || !path.contains("/video-opt/") {
        return path.to_string();
    }
    state
        .with_db(|connection| skwd_wall_core::db::tinier_convert_src(connection, path))
        .ok()
        .flatten()
        .unwrap_or_else(|| path.to_string())
}

pub(super) fn current_of(
    path: &str,
    we_id: &str,
    assigns: &std::collections::HashMap<String, String>,
    name: &str,
) -> String {
    if !path.is_empty() {
        return path.to_string();
    }
    if !we_id.is_empty() {
        return we_id.to_string();
    }
    assigns.get(name).cloned().unwrap_or_default()
}

pub(super) fn theme_preview(state: &Arc<WallState>, req: &Request) -> Response {
    state.reload_config();
    let image = req.str_param("image", "");
    if image.is_empty() {
        return Response::err(req.id, 1, "no image to preview");
    }
    let cfg = state.config();
    let palette = skwd_wall_core::bridge_preview::cached_palette(state, image);
    let colors =
        palette.as_ref().map(skwd_wall_core::theme::swatch_from_palette).unwrap_or_default();
    Response::ok(
        req.id,
        json!({
            "backend": skwd_wall_core::theme::effective_backend(&cfg),
            "colors": colors,
            "palette": palette,
        }),
    )
}

pub(super) fn theme_previews(state: &Arc<WallState>, req: &Request) -> Response {
    state.reload_config();
    let image = req
        .opt_str("image")
        .filter(|image| !image.is_empty())
        .map(str::to_string)
        .or_else(|| state.theme().source());
    let Some(image) = image else {
        return Response::err(req.id, 1, "no current wallpaper for theme previews");
    };
    let cfg = state.config();
    let detected = skwd_wall_core::theme::available_backends(&cfg);
    let backends = skwd_wall_core::theme::previewable_backends(&detected);
    let requested = req.opt_str("backend").unwrap_or_default();
    let effective = skwd_wall_core::theme::effective_backend(&cfg);
    let backend = if backends.contains(&requested) {
        requested
    } else if backends.contains(&effective.as_str()) {
        effective.as_str()
    } else {
        backends.first().copied().unwrap_or("skwd-iris")
    };
    let previews = skwd_wall_core::theme::audition_profiles(&cfg, &image, backend)
        .into_iter()
        .map(|profile| {
            json!({
                "backend": backend,
                "key": profile.key,
                "value": profile.value,
                "label": profile.label,
                "palette": profile.palette,
            })
        })
        .collect::<Vec<_>>();
    if previews.is_empty() {
        return Response::err(req.id, 1, format!("could not derive {backend} theme previews"));
    }
    Response::ok(req.id, json!({"backend": backend, "backends": backends, "previews": previews}))
}

pub(super) fn clear_data(ctx: &Ctx, req: &Request) -> Response {
    let Ctx { state, events, workers, stats, .. } = ctx;
    let _ = state.with_db(db::clear_cache);
    for dir in [
        skwd_wall_core::paths::thumbs_dir(),
        skwd_wall_core::paths::thumbs_sm_dir(),
        skwd_wall_core::paths::video_thumbs_dir(),
    ] {
        let _ = std::fs::remove_dir_all(&dir);
    }
    stats.set_task("scanning");
    workers.scan(&[], None);
    events.publish(ev::CLEARED, json!({}));
    Response::ok(req.id, json!({"started": true}))
}

pub(super) fn wall_apply(ctx: &Ctx, req: &Request) -> Response {
    let Ctx { state, wallpaper, history, events, workers, stats, .. } = ctx;
    state.reload_config();
    let ty = req.str_param("type", wall_proto::kind::STATIC);
    let path = req.str_param("path", "");
    let we_id = req.str_param("we_id", "");
    if ty == wall_proto::kind::WE && we_id.is_empty() {
        return crate::infrastructure::rpc::fail_msg(stats, req.id, -32602, "missing we_id");
    }
    if ty != wall_proto::kind::WE && path.is_empty() {
        return crate::infrastructure::rpc::fail_msg(stats, req.id, -32602, "missing path");
    }
    let output = req.str_param("output", "*");
    let (def_mute, def_vol) = {
        let cfg = state.config();
        skwd_wall_core::audio::resolve_defaults(
            &cfg.cache_dir(),
            output,
            cfg.renderer().mute(),
            cfg.renderer().volume(),
        )
    };
    let mute = req.params.get("mute").and_then(serde_json::Value::as_bool).unwrap_or(def_mute);
    let volume = req
        .params
        .get("volume")
        .and_then(serde_json::Value::as_u64)
        .map_or(def_vol, |vol| vol as u32);
    let notify = req.bool_param("notify", true);
    let no_transition = req.bool_param("no_transition", false);
    let transition_shader = req
        .params
        .get("transition_shader")
        .and_then(serde_json::Value::as_str)
        .filter(|shader| {
            !shader.is_empty()
                && shader.len() <= 64
                && shader.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
        })
        .map(str::to_string);
    let transition_override = crate::composition::apply::TransitionOverride {
        enabled: req.params.get("transition").and_then(serde_json::Value::as_bool),
        shader: transition_shader,
        duration_ms: req
            .params
            .get("transition_duration_ms")
            .and_then(serde_json::Value::as_u64)
            .map(|duration| duration.clamp(50, 10_000)),
    };
    let transition_override = (transition_override.enabled.is_some()
        || transition_override.shader.is_some()
        || transition_override.duration_ms.is_some())
    .then_some(transition_override);
    let source = match req.str_param("source", "user") {
        "random" => ApplySource::Random,
        _ if req.bool_param("override_locks", false) => ApplySource::UserOverride,
        _ => ApplySource::User,
    };
    if ty == wall_proto::kind::VIDEO
        && state.config().renderer().video_engine() == "tinier"
        && !output.starts_with("@monitor:")
        && crate::infrastructure::media_paths::tinier_video(state, path).is_none()
    {
        let generation = state.apply().next_generation();
        let preparation = workers.prepare_tinier(path);
        let task_id = preparation.task_id.clone();
        let deferred_state = Arc::clone(state);
        let deferred_wallpaper = Arc::clone(wallpaper);
        let deferred_history = Arc::clone(history);
        let deferred_events = Arc::clone(events);
        let deferred_stats = Arc::clone(stats);
        let path = path.to_string();
        let deferred_output = output.to_string();
        let transition_override = transition_override.clone();
        let request_id = req.id;
        thread::spawn(move || {
            let result = preparation
                .result
                .recv()
                .map_err(|error| error.to_string())
                .and_then(|inner| inner);
            match preparation_terminal(generation, deferred_state.apply().generation(), result) {
                DeferredTerminal::Superseded => {
                    log::info!("tinier preparation superseded: {path}");
                    return;
                }
                DeferredTerminal::Cancelled => return,
                DeferredTerminal::Failure(detail) => {
                    deferred_stats.error();
                    deferred_events.publish(
                        ev::APPLY_RESULT,
                        json!({
                            "request_id": request_id,
                            "ok": false,
                            "output": deferred_output,
                            "error_kind": "decode_failed",
                            "detail": detail,
                        }),
                    );
                    return;
                }
                DeferredTerminal::Success => {}
            }
            let applied = apply_core(
                &deferred_state,
                deferred_wallpaper.as_ref(),
                deferred_history.as_ref(),
                deferred_events.as_ref(),
                &deferred_stats,
                wall_proto::kind::VIDEO,
                &path,
                "",
                mute,
                volume,
                source,
                &deferred_output,
                notify,
                no_transition,
                transition_override.as_ref(),
                Some(generation),
            );
            match apply_terminal(applied) {
                DeferredTerminal::Success => deferred_events.publish(
                    ev::APPLY_RESULT,
                    json!({ "request_id": request_id, "ok": true, "output": deferred_output }),
                ),
                DeferredTerminal::Superseded => {
                    log::info!("tinier apply superseded after preparation: {path}");
                }
                DeferredTerminal::Failure(detail) => {
                    deferred_stats.error();
                    deferred_events.publish(
                        ev::APPLY_RESULT,
                        json!({
                            "request_id": request_id,
                            "ok": false,
                            "output": deferred_output,
                            "error_kind": classify_apply_error(&detail),
                            "detail": detail,
                        }),
                    );
                }
                DeferredTerminal::Cancelled => unreachable!("apply execution is not cancellable"),
            }
        });
        events.publish(
            ev::APPLY_RESULT,
            json!({ "request_id": req.id, "ok": true, "output": output, "queued": true }),
        );
        return Response::ok(
            req.id,
            json!({"queued": true, "task_id": task_id, "status": "preparing"}),
        );
    }
    match apply_core(
        state,
        wallpaper.as_ref(),
        history.as_ref(),
        events.as_ref(),
        stats,
        ty,
        path,
        we_id,
        mute,
        volume,
        source,
        output,
        notify,
        no_transition,
        transition_override.as_ref(),
        None,
    ) {
        Ok(val) => {
            events.publish(
                ev::APPLY_RESULT,
                json!({ "request_id": req.id, "ok": true, "output": output }),
            );
            Response::ok(req.id, val)
        }
        Err(err) => {
            stats.error();
            let detail = err.to_string();
            events.publish(
                ev::APPLY_RESULT,
                json!({
                    "request_id": req.id,
                    "ok": false,
                    "output": output,
                    "error_kind": classify_apply_error(&detail),
                    "detail": detail,
                }),
            );
            Response::err(req.id, -1, detail)
        }
    }
}

pub(super) fn set_audio(
    state: &Arc<WallState>,
    renderers: &dyn skwd_wall_core::backend::renderers::RendererSupervision,
    wallpaper: &dyn skwd_wall_core::backend::wallpaper::WallpaperApplication,
    req: &Request,
) -> Response {
    state.reload_config();
    let _apply = state.apply().lock();
    let mute = req.params.get("mute").and_then(serde_json::Value::as_bool);
    let volume = req
        .params
        .get("volume")
        .and_then(serde_json::Value::as_u64)
        .map(|vol| (vol as u32).min(100));
    let outputs: Option<Vec<String>> = req
        .params
        .get("outputs")
        .and_then(serde_json::Value::as_array)
        .map(|arr| arr.iter().filter_map(|val| val.as_str().map(String::from)).collect());
    let filter = outputs.as_deref();
    renderers.send_audio(filter, mute, volume);
    let cache = state.config().cache_dir();
    if filter.is_some() {
        let names = skwd_wall_core::outputs::names();
        if !names.is_empty() {
            skwd_wall_core::audio::expand_wildcard(&cache, &names);
        }
    }
    skwd_wall_core::audio::update_audio(&cache, filter, mute, volume);
    skwd_wall_core::audio::mute_dedup_losers(state, &cache);
    // Dedup's targeted mutes also reach the shared `*` renderer; restore the group state after.
    if renderers.has_shared_video() {
        let (default_mute, default_volume) = {
            let config = state.config();
            (config.renderer().mute(), config.renderer().volume())
        };
        let (shared_mute, shared_volume) = skwd_wall_core::audio::carried_audio(
            &skwd_wall_core::audio::read_state(&cache),
            "*",
            default_mute,
            default_volume,
        );
        renderers.send_shared_video_audio(shared_mute, shared_volume);
    }
    if let Err(err) = wallpaper.reload_we() {
        log::warn!("set_audio: WE audio reload failed: {err}");
    }
    if skwd_wall_core::plasma::available()
        && let Err(error) = skwd_wall_core::plasma::apply_current(state)
    {
        log::warn!("set_audio: Plasma update failed: {error:#}");
    }
    log::info!("set_audio: mute={mute:?} volume={volume:?} outputs={outputs:?}");
    Response::ok(req.id, json!({"ok": true}))
}

pub(super) fn shell_preview(state: &Arc<WallState>, req: &Request, stats: &Arc<Stats>) -> Response {
    state.reload_config();
    let path = req.str_param("path", "").to_string();
    let cfg = state.config();
    if path.is_empty() || !std::path::Path::new(&path).is_file() {
        return crate::infrastructure::rpc::fail_msg(
            stats,
            req.id,
            -1,
            "path missing or not a file".to_string(),
        );
    }
    let backend = cfg.theme().backend();
    let armed = match backend.as_str() {
        "noctalia" => cfg.theme().noctalia_hover_preview(),
        "dms" => cfg.theme().dms_hover_preview(),
        "static" | "off" => false,
        _ => true,
    };
    if !armed {
        return crate::infrastructure::rpc::fail_msg(
            stats,
            req.id,
            -1,
            "shell hover preview disabled".to_string(),
        );
    }
    if let Some(arm) = skwd_wall_core::theme_sink::active(backend.as_str()).arm {
        arm(state);
    }
    let generation = state.theme().bump_shell_preview();
    let owned = Arc::clone(state);
    thread::spawn(move || shell_preview_run(&owned, &backend, &path, generation));
    Response::ok(req.id, json!({"queued": true}))
}

pub(super) fn shell_preview_run(
    state: &Arc<WallState>,
    backend: &str,
    path: &str,
    generation: u64,
) {
    let res = (skwd_wall_core::theme_sink::active(backend).preview)(state, path, generation);
    if let Err(err) = res {
        log::warn!("{backend} preview: {err:#}");
    }
}

pub(super) fn wall_reload_we(ctx: &Ctx, req: &Request) -> Response {
    let Ctx { state, wallpaper, stats, .. } = ctx;
    state.reload_config();
    let _apply = state.apply().lock();
    if skwd_wall_core::plasma::available() {
        return match skwd_wall_core::plasma::apply_current(state) {
            Ok(()) => Response::ok(req.id, json!({"reloaded": true})),
            Err(error) => fail(stats, req.id, error),
        };
    }
    match reload_current_we(state, wallpaper.as_ref()) {
        Ok(reloaded) => Response::ok(req.id, json!({"reloaded": reloaded})),
        Err(err) => fail(stats, req.id, err),
    }
}

pub(super) fn wall_we_properties(ctx: &Ctx, req: &Request) -> Response {
    let we_id = req.str_param("we_id", "");
    if !skwd_wall_core::we::valid_we_id(we_id) {
        return Response::ok(req.id, json!({"we_id": we_id, "properties": []}));
    }
    let rows = skwd_wall_core::we::scene_properties(&ctx.state, we_id);
    Response::ok(req.id, json!({"we_id": we_id, "properties": rows}))
}

pub(super) fn wall_set_we_property(ctx: &Ctx, req: &Request) -> Response {
    let Ctx { state, stats, .. } = ctx;
    let we_id = req.str_param("we_id", "").to_string();
    let name = req.str_param("name", "").to_string();
    if !skwd_wall_core::we::valid_we_id(&we_id) {
        return fail(stats, req.id, anyhow::anyhow!("invalid Wallpaper Engine id"));
    }
    let reset = req.bool_param("reset", false);
    if reset {
        if let Err(err) = state.with_db(|conn| db::clear_we_properties(conn, &we_id)) {
            return fail(stats, req.id, err);
        }
    } else {
        if !db::valid_property_name(&name) {
            return fail(stats, req.id, anyhow::anyhow!("invalid scene property name"));
        }
        let value = req.params.get("value").cloned();
        let stored = match &value {
            Some(serde_json::Value::Null) | None => None,
            Some(value) => Some(value.clone()),
        };
        if stored.is_none() {
            if let Err(err) = state.with_db(|conn| db::set_we_property(conn, &we_id, &name, None)) {
                return fail(stats, req.id, err);
            }
        } else {
            let existing =
                state.with_db(|conn| Ok(db::we_properties(conn, &we_id))).unwrap_or_default();
            if !existing.contains_key(&name) && existing.len() >= db::MAX_WE_PROPERTIES {
                return fail(
                    stats,
                    req.id,
                    anyhow::anyhow!("scene property limit of {} reached", db::MAX_WE_PROPERTIES),
                );
            }
            if let Err(err) =
                state.with_db(|conn| db::set_we_property(conn, &we_id, &name, stored.as_ref()))
            {
                return fail(stats, req.id, err);
            }
        }
    }
    let rows = skwd_wall_core::we::scene_properties(state, &we_id);
    let applied = reapply_scene_if_current(ctx, &we_id);
    Response::ok(req.id, json!({"we_id": we_id, "properties": rows, "reapplied": applied}))
}

fn reapply_scene_if_current(ctx: &Ctx, we_id: &str) -> bool {
    let Ctx { state, wallpaper, .. } = ctx;
    let _apply = state.apply().lock();
    let current = crate::infrastructure::persistence::current_entry();
    let applied = current
        .as_ref()
        .is_some_and(|entry| entry.ty == wall_proto::kind::WE && entry.we_id == we_id);
    if !applied {
        return false;
    }
    if skwd_wall_core::plasma::available() {
        return skwd_wall_core::plasma::apply_current(state).is_ok();
    }
    matches!(reload_current_we(state, wallpaper.as_ref()), Ok(true))
}

pub(super) fn set_favourite(state: &Arc<WallState>, req: &Request, stats: &Arc<Stats>) -> Response {
    let key = req.str_param("key", "");
    let fav = req.bool_param("favourite", true);
    match state.with_db(|conn| db::set_favourite(conn, key, fav)) {
        Ok(changed) => {
            Response::ok(req.id, json!({"key": key, "favourite": fav, "changed": changed}))
        }
        Err(err) => fail(stats, req.id, err),
    }
}

#[cfg(test)]
mod deferred_terminal_tests {
    use super::*;

    #[test]
    fn tinier_terminal_results_distinguish_success_failure_cancel_and_supersession() {
        assert_eq!(preparation_terminal(4, 4, Ok(())), DeferredTerminal::Success);
        assert_eq!(
            preparation_terminal(4, 4, Err("Preparation cancelled".into())),
            DeferredTerminal::Cancelled
        );
        assert_eq!(preparation_terminal(4, 5, Ok(())), DeferredTerminal::Superseded);
        assert_eq!(
            preparation_terminal(4, 4, Err("decode broke".into())),
            DeferredTerminal::Failure("decode broke".into())
        );
        assert_eq!(apply_terminal(Ok(json!({}))), DeferredTerminal::Success);
        assert_eq!(
            apply_terminal(Err(anyhow::Error::new(crate::composition::apply::SupersededApply))),
            DeferredTerminal::Superseded
        );
        assert_eq!(
            apply_terminal(Err(anyhow::anyhow!("superseded apply"))),
            DeferredTerminal::Failure("superseded apply".into())
        );
    }
}
