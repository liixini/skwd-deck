use std::sync::Arc;
use std::time::Instant;

use serde_json::json;
use skwd_wall_core::WallState;
use skwd_wall_core::backend::wallpaper::{
    ApplyOutputRequest, ApplyStaticRequest, ApplyVideoRequest, OutputTransitionRequest,
    VideoTransitionRequest, WallpaperApplication,
};
use wall_proto::ev;

use crate::backend::events::EventPublisher;
use crate::backend::history::{ApplySource, HistoryRepository};
use crate::infrastructure::media_paths::{VideoRoute, static_thumb, video_route, video_thumb};
use crate::infrastructure::stats::Stats;

mod phases;
use phases::{ApplyDecision, ExecutionReceipt, MediaKind};

#[derive(Debug)]
pub(crate) struct SupersededApply;

impl std::fmt::Display for SupersededApply {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("superseded apply")
    }
}

impl std::error::Error for SupersededApply {}

#[derive(Clone, Debug, Default)]
pub(crate) struct TransitionOverride {
    pub(crate) enabled: Option<bool>,
    pub(crate) shader: Option<String>,
    pub(crate) duration_ms: Option<u64>,
}

struct RendererApplyWindow<'a> {
    renderers: &'a skwd_wall_core::infrastructure::renderers::RendererSupervisor,
}

struct ApplyPolicyWindow<'a> {
    state: &'a WallState,
}

fn theme_delay() -> std::time::Duration {
    std::time::Duration::ZERO
}

impl<'a> RendererApplyWindow<'a> {
    fn new(renderers: &'a skwd_wall_core::infrastructure::renderers::RendererSupervisor) -> Self {
        renderers.begin_apply();
        Self { renderers }
    }
}

impl Drop for RendererApplyWindow<'_> {
    fn drop(&mut self) {
        self.renderers.end_apply();
    }
}

impl<'a> ApplyPolicyWindow<'a> {
    fn new(state: &'a WallState, no_transition: bool, workspace: bool) -> Self {
        state.apply().set_no_transition(no_transition);
        if !workspace {
            state.apply().set_swap_slide(None);
        }
        Self { state }
    }
}

impl Drop for ApplyPolicyWindow<'_> {
    fn drop(&mut self) {
        self.state.apply().set_no_transition(false);
        self.state.apply().set_swap_slide(None);
    }
}

pub(crate) fn split_locked_outputs(
    outputs: Vec<String>,
    locked: &[String],
) -> (Vec<String>, Vec<String>) {
    outputs.into_iter().partition(|output| !locked.contains(output))
}

pub(crate) fn stage_unlocked_media_outputs(
    state: &WallState,
    outputs: &[String],
    kind: &str,
    path: &str,
    we_id: &str,
    mute: bool,
    volume: u32,
) {
    let eligible =
        matches!(kind, wall_proto::kind::STATIC | wall_proto::kind::VIDEO | wall_proto::kind::WE);
    if state.config().pick_only_mode() || !eligible {
        return;
    }
    let cache = state.config().cache_dir();
    let mut staged = skwd_wall_core::audio::read_state(&cache);
    let map = staged.as_object_mut().expect("audio state is always an object");
    if let Some(wildcard) = map.remove("*") {
        for output in skwd_wall_core::outputs::names() {
            map.entry(output).or_insert_with(|| wildcard.clone());
        }
    }
    for output in outputs {
        map.insert(output.clone(), skwd_wall_core::audio::entry(kind, path, we_id, mute, volume));
    }
    skwd_wall_core::audio::write_state(&cache, &staged);
}

pub(crate) fn apply_core(
    state: &Arc<WallState>,
    application: &dyn WallpaperApplication,
    history: &dyn HistoryRepository,
    publisher: &dyn EventPublisher,
    stats: &Stats,
    kind: &str,
    path: &str,
    we_id: &str,
    mute: bool,
    volume: u32,
    source: ApplySource,
    output: &str,
    notify: bool,
    no_transition: bool,
    transition_override: Option<&TransitionOverride>,
    expected_generation: Option<u64>,
) -> anyhow::Result<serde_json::Value> {
    if let Some(id) = output.strip_prefix("@monitor:") {
        let _apply = state.apply().lock();
        let connector = crate::infrastructure::restore_policy::remembered_monitor_connector(id)
            .ok_or_else(|| anyhow::anyhow!("remembered monitor is unavailable"))?;
        if source.respects_output_locks()
            && !connector.is_empty()
            && state.config().display().output_locked(&connector)
        {
            log::info!("apply: skipped locked remembered output {connector}");
            return Ok(json!({"applied": "", "noop": true, "locked": connector}));
        }
        crate::infrastructure::restore_policy::assign_remembered_monitor(id, kind, path, we_id)
            .ok_or_else(|| anyhow::anyhow!("remembered monitor is unavailable"))?;
        publisher
            .publish(ev::OUTPUTS_CHANGED, json!({ "outputs": skwd_wall_core::outputs::names() }));
        log::info!("apply: remembered wallpaper for offline monitor {id}");
        return Ok(json!({
            "applied": if kind == wall_proto::kind::WE { we_id } else { path },
            "offline": true
        }));
    }
    if source.respects_output_locks() {
        if output != "*" && state.config().display().output_locked(output) {
            log::info!("apply: skipped locked output {output}");
            return Ok(json!({"applied": "", "noop": true, "locked": output}));
        }
        if output == "*" {
            let locked = state.config().display().locked_outputs();
            let (unlocked, preserved) =
                split_locked_outputs(skwd_wall_core::outputs::names(), &locked);
            if !preserved.is_empty() {
                let _apply = state.apply().lock();
                return apply_locked_fanout(
                    state,
                    application,
                    history,
                    publisher,
                    stats,
                    kind,
                    path,
                    we_id,
                    mute,
                    volume,
                    source,
                    &unlocked,
                    &preserved,
                    notify,
                    no_transition,
                    transition_override,
                    expected_generation,
                );
            }
        }
    }
    let _apply = state.apply().lock();
    apply_core_locked(
        state,
        application,
        history,
        publisher,
        stats,
        kind,
        path,
        we_id,
        mute,
        volume,
        source,
        output,
        notify,
        no_transition,
        transition_override,
        GenerationRequest::Claim(expected_generation),
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn apply_locked_fanout(
    state: &Arc<WallState>,
    application: &dyn WallpaperApplication,
    history: &dyn HistoryRepository,
    publisher: &dyn EventPublisher,
    stats: &Stats,
    kind: &str,
    path: &str,
    we_id: &str,
    mute: bool,
    volume: u32,
    source: ApplySource,
    unlocked: &[String],
    preserved: &[String],
    notify: bool,
    no_transition: bool,
    transition_override: Option<&TransitionOverride>,
    expected_generation: Option<u64>,
) -> anyhow::Result<serde_json::Value> {
    if unlocked.is_empty() {
        return Ok(json!({"applied": [], "locked": preserved}));
    }
    let generation = claim_generation(state, expected_generation)?;
    let cache = state.config().cache_dir();
    let previous_desired = skwd_wall_core::audio::read_state(&cache);
    let staged_path = if kind == wall_proto::kind::VIDEO
        && state.config().renderer().video_engine() == "tinier"
    {
        crate::infrastructure::media_paths::tinier_video(state, path)
            .map_or_else(|| path.to_string(), |video| video.path)
    } else {
        path.to_string()
    };
    let render_output = &unlocked[0];
    state.apply().set_transition_source(
        skwd_wall_core::backend::wallpaper::ApplyRuntime::source_we_in(
            &previous_desired,
            &skwd_wall_core::outputs::names(),
            render_output,
            Some(&state.config().we_dir()),
        ),
    );
    stage_unlocked_media_outputs(state, unlocked, kind, &staged_path, we_id, mute, volume);
    let result = apply_core_locked(
        state,
        application,
        history,
        publisher,
        stats,
        kind,
        path,
        we_id,
        mute,
        volume,
        source,
        render_output,
        notify,
        no_transition,
        transition_override,
        GenerationRequest::Reserved(generation),
        Some(unlocked),
    );
    if let Err(error) = result {
        skwd_wall_core::audio::write_state(&cache, &previous_desired);
        if let Err(rollback) =
            restore_renderer_snapshot(state, application, render_output, &previous_desired)
        {
            log::warn!("apply rollback for {render_output} failed: {rollback:#}");
        }
        skwd_wall_core::audio::write_state(&cache, &previous_desired);
        return Err(error);
    }
    log::info!("apply: updated unlocked outputs {unlocked:?}; preserved {preserved:?}");
    Ok(json!({"applied": unlocked, "locked": preserved}))
}

fn restore_renderer_snapshot(
    state: &WallState,
    application: &dyn WallpaperApplication,
    output: &str,
    desired: &serde_json::Value,
) -> anyhow::Result<()> {
    let Some((restore_output, entry)) = desired
        .get(output)
        .map(|entry| (output, entry))
        .or_else(|| desired.get("*").map(|entry| ("*", entry)))
    else {
        return Ok(());
    };
    let kind = entry.get("type").and_then(serde_json::Value::as_str).unwrap_or("");
    let path = entry.get("path").and_then(serde_json::Value::as_str).unwrap_or("");
    let we_id = entry.get("we_id").and_then(serde_json::Value::as_str).unwrap_or("");
    let mute = entry.get("mute").and_then(serde_json::Value::as_bool).unwrap_or(true);
    let volume = entry.get("volume").and_then(serde_json::Value::as_u64).unwrap_or(100) as u32;
    if !matches!(kind, wall_proto::kind::STATIC | wall_proto::kind::VIDEO | wall_proto::kind::WE) {
        return Ok(());
    }
    application.apply_output(ApplyOutputRequest {
        output: restore_output,
        kind,
        path,
        we_id,
        fill_mode: &state.config().display().fill_mode_for(output),
        mute,
        volume,
        frame_rate: None,
        transition: None,
    })
}

#[derive(Clone, Copy)]
enum GenerationRequest {
    Claim(Option<u64>),
    Reserved(u64),
}

fn claim_generation(state: &WallState, expected: Option<u64>) -> anyhow::Result<u64> {
    match expected {
        Some(expected) => state
            .apply()
            .claim_generation(expected)
            .ok_or_else(|| anyhow::Error::new(SupersededApply)),
        None => Ok(state.apply().next_generation()),
    }
}

fn apply_core_locked(
    state: &Arc<WallState>,
    application: &dyn WallpaperApplication,
    history: &dyn HistoryRepository,
    publisher: &dyn EventPublisher,
    stats: &Stats,
    kind: &str,
    path: &str,
    we_id: &str,
    mute: bool,
    volume: u32,
    source: ApplySource,
    output: &str,
    notify: bool,
    no_transition: bool,
    transition_override: Option<&TransitionOverride>,
    generation_request: GenerationRequest,
    committed_outputs: Option<&[String]>,
) -> anyhow::Result<serde_json::Value> {
    let generation = match generation_request {
        GenerationRequest::Claim(expected) => claim_generation(state, expected)?,
        GenerationRequest::Reserved(generation) => generation,
    };
    let workspace = matches!(source, ApplySource::Workspace);
    let _policy = ApplyPolicyWindow::new(state, workspace || no_transition, workspace);
    let started = Instant::now();
    let random = source.broadcast_random();
    let fill = if output == "*" {
        state.config().display().fill_mode()
    } else {
        state.config().display().fill_mode_for(output)
    };
    let mixed = output == "*" && state.config().display().fill_overrides_active();
    let pick_only = state.config().pick_only_mode();
    let prior_entry = if source.records() && state.config().history_enabled() {
        crate::infrastructure::persistence::current_entry()
    } else {
        None
    };
    if output == "*"
        && !mixed
        && !pick_only
        && crate::infrastructure::persistence::last_matches(kind, path, we_id, mute, volume)
        && state.apply().render_fill() == fill
        && state.renderers().renderer_alive(kind)
        && skwd_wall_core::apply::renderer_policy_matches(state, kind)
        && (kind != wall_proto::kind::WE
            || skwd_wall_core::apply::native_scene_properties_match(state, we_id))
        && state.apply().persisted_uniform(
            &state.config().cache_dir(),
            &skwd_wall_core::outputs::names(),
            kind,
            path,
            we_id,
        )
        && state.renderers().no_per_output_renderers()
    {
        let applied = if kind == wall_proto::kind::WE { we_id } else { path };
        log::info!("apply: no-op, {kind} {applied} already applied");
        crate::composition::history::note_apply_source(source);
        crate::infrastructure::lock_screen::request_follow_sync(state);
        return Ok(json!({"applied": applied, "noop": true}));
    }
    let library_key = applied_library_key(state, kind, path, we_id);
    let _renderer_apply = RendererApplyWindow::new(state.renderers());
    state.apply().set_render_fill(&fill);
    if pick_only && state.config().post_processing().is_empty() {
        log::warn!(
            "apply gen={generation}: pickOnlyMode is ON with no post-processing commands - wallpaper will NOT change. Turn off 'Disable internal wallpaper application' in EXTERNAL settings, or add a setter command."
        );
    }
    let media = match kind {
        wall_proto::kind::STATIC => MediaKind::Static,
        wall_proto::kind::VIDEO => MediaKind::Video,
        wall_proto::kind::WE => MediaKind::WallpaperEngine,
        other => anyhow::bail!("apply type '{other}' not supported"),
    };
    let decision = ApplyDecision {
        generation,
        media,
        path: path.to_string(),
        we_id: we_id.to_string(),
        output: output.to_string(),
        committed_outputs: committed_outputs
            .map_or_else(|| vec![output.to_string()], <[String]>::to_vec),
        mute,
        volume,
        source,
        notify,
        random,
        library_key,
        prior_entry,
    };
    let execution = match kind {
        wall_proto::kind::STATIC => apply_static_arm(
            state,
            application,
            &decision,
            &fill,
            pick_only,
            started,
            transition_override,
        ),
        wall_proto::kind::VIDEO => {
            apply_video_arm(state, application, &decision, &fill, pick_only, transition_override)
        }
        wall_proto::kind::WE => apply_we_arm(state, application, &decision, &fill, pick_only),
        _ => unreachable!("media decision validates kind"),
    };
    if let Err(error) = &execution {
        let target = if path.is_empty() { we_id } else { path };
        log::warn!("apply gen={generation}: {kind} to={target} output={output} failed: {error:#}");
    }
    Ok(execution?.commit(state)?.publish(state, history, publisher, stats))
}

fn apply_static_arm(
    state: &Arc<WallState>,
    application: &dyn WallpaperApplication,
    decision: &ApplyDecision,
    fill: &str,
    pick_only: bool,
    started: Instant,
    transition_override: Option<&TransitionOverride>,
) -> anyhow::Result<ExecutionReceipt> {
    let path = &decision.path;
    let output = &decision.output;
    let generation = decision.generation;
    let (mut transitions_enabled, mut shader, mut duration_ms) = {
        let config = state.config();
        (
            config.transition().active(),
            config.transition().shader(),
            config.transition().duration_ms(),
        )
    };
    if let Some(transition_override) = transition_override {
        transitions_enabled = transition_override.enabled.unwrap_or(transitions_enabled);
        if let Some(value) = transition_override.shader.as_ref() {
            shader.clone_from(value);
        }
        duration_ms = transition_override.duration_ms.unwrap_or(duration_ms);
    }
    transitions_enabled &= !state.apply().no_transition();
    if !pick_only {
        let from = state
            .apply()
            .take_transition_source()
            .or_else(|| {
                state.apply().current_source_we(
                    &state.config().cache_dir(),
                    &skwd_wall_core::outputs::names(),
                    output,
                    Some(&state.config().we_dir()),
                )
            })
            .or_else(crate::infrastructure::persistence::last_any_thumb);
        log::info!(
            "apply gen={generation}: static to={path} output={output} (from={from:?} trans_on={transitions_enabled})"
        );
        application.apply_static(ApplyStaticRequest {
            output,
            path,
            fill_mode: fill,
            from: from.as_deref(),
            transition: transitions_enabled,
            shader: &shader,
            duration_ms,
        })?;
        log::info!(
            "apply gen={generation}: static render handoff in {} ms (engine={})",
            started.elapsed().as_millis(),
            state.config().renderer().engine()
        );
        if transitions_enabled {
            let sup = state.renderers_shared();
            let delay = std::time::Duration::from_millis(duration_ms.saturating_add(700));
            std::thread::spawn(move || {
                std::thread::sleep(delay);
                sup.reap_exited();
            });
        }
    }
    let theme_source = static_thumb(state, path).unwrap_or_else(|| path.clone());
    Ok(ExecutionReceipt::new(decision.clone(), Some(theme_source), path.clone()))
}

fn apply_video_arm(
    state: &Arc<WallState>,
    application: &dyn WallpaperApplication,
    decision: &ApplyDecision,
    fill: &str,
    pick_only: bool,
    transition_override: Option<&TransitionOverride>,
) -> anyhow::Result<ExecutionReceipt> {
    let path = &decision.path;
    let output = &decision.output;
    let generation = decision.generation;
    if !pick_only {
        log::info!("apply gen={generation}: video to={path} output={output}");
        let render_started = std::time::Instant::now();
        video_render(
            state,
            application,
            output,
            path,
            fill,
            decision.mute,
            decision.volume,
            transition_override,
        )?;
        log::info!(
            "apply gen={generation}: video render handoff in {} ms",
            render_started.elapsed().as_millis()
        );
    }
    let thumb = video_thumb(state, path);
    let persisted_thumb = thumb.clone().unwrap_or_else(|| path.clone());
    Ok(ExecutionReceipt::new(decision.clone(), thumb, persisted_thumb))
}

fn video_render(
    state: &Arc<WallState>,
    application: &dyn WallpaperApplication,
    output: &str,
    path: &str,
    fill: &str,
    mute: bool,
    volume: u32,
    transition_override: Option<&TransitionOverride>,
) -> anyhow::Result<()> {
    let (mut transitions_enabled, mut shader, mut duration_ms) = {
        let config = state.config();
        (
            config.transition().active(),
            config.transition().shader(),
            config.transition().duration_ms(),
        )
    };
    if let Some(transition_override) = transition_override {
        transitions_enabled = transition_override.enabled.unwrap_or(transitions_enabled);
        if let Some(value) = transition_override.shader.as_ref() {
            shader.clone_from(value);
        }
        duration_ms = transition_override.duration_ms.unwrap_or(duration_ms);
    }
    transitions_enabled &= !state.apply().no_transition();
    if transition_override.is_some() {
        log::info!(
            "video transition override: output={output} enabled={transitions_enabled} shader={shader} duration_ms={duration_ms}"
        );
    }
    let tinier = (state.config().renderer().video_engine() == "tinier")
        .then(|| crate::infrastructure::media_paths::tinier_video(state, path))
        .flatten();
    if state.config().renderer().video_engine() == "tinier" && tinier.is_none() {
        anyhow::bail!("Tinier AV1 preparation is not ready for {path}");
    }
    let render_path = tinier.as_ref().map_or_else(|| path.to_string(), |video| video.path.clone());
    let frame_rate = tinier.as_ref().map(|video| video.frame_rate.as_str());
    if output != "*" {
        return application.apply_output(ApplyOutputRequest {
            output,
            kind: wall_proto::kind::VIDEO,
            path: &render_path,
            we_id: "",
            fill_mode: fill,
            mute,
            volume,
            frame_rate,
            transition: Some(OutputTransitionRequest {
                enabled: transitions_enabled,
                shader: &shader,
                duration_ms,
            }),
        });
    }
    if frame_rate.is_some() {
        let outputs = skwd_wall_core::outputs::names();
        if outputs.is_empty() {
            anyhow::bail!("Tinier requires at least one named output");
        }
        for output in outputs {
            application.apply_output(ApplyOutputRequest {
                output: &output,
                kind: wall_proto::kind::VIDEO,
                path: &render_path,
                we_id: "",
                fill_mode: fill,
                mute,
                volume,
                frame_rate,
                transition: None,
            })?;
        }
        return Ok(());
    }
    let from = crate::infrastructure::persistence::last_any_source();
    let thumb = video_thumb(state, path);
    match video_route(transitions_enabled, from.as_deref(), &render_path, thumb.as_deref()) {
        VideoRoute::Transition(source) => {
            application.apply_video_transition(VideoTransitionRequest {
                from: source,
                to: &render_path,
                fill_mode: fill,
                shader: &shader,
                duration_ms,
                mute,
                volume,
            })
        }
        VideoRoute::Plain => application.apply_video(ApplyVideoRequest {
            output: "*",
            path: &render_path,
            fill_mode: fill,
            mute,
            volume,
        }),
    }
}

fn apply_we_arm(
    state: &Arc<WallState>,
    application: &dyn WallpaperApplication,
    decision: &ApplyDecision,
    fill: &str,
    pick_only: bool,
) -> anyhow::Result<ExecutionReceipt> {
    let we_id = &decision.we_id;
    let output = &decision.output;
    if !skwd_wall_core::we::valid_we_id(we_id) {
        anyhow::bail!("invalid WE id: {we_id}");
    }
    let preview = if pick_only {
        let item_directory = state.config().we_dir().join(we_id);
        skwd_wall_core::we::find_preview(&item_directory).map(|path| path.display().to_string())
    } else if output != "*" {
        we_render_output(state, application, we_id, output, fill, decision.mute, decision.volume)?
    } else {
        application.apply_we(we_id)?
    };
    let persisted_thumb = preview.clone().unwrap_or_default();
    Ok(ExecutionReceipt::new(decision.clone(), preview, persisted_thumb))
}

fn we_render_output(
    state: &Arc<WallState>,
    application: &dyn WallpaperApplication,
    we_id: &str,
    output: &str,
    fill: &str,
    mute: bool,
    volume: u32,
) -> anyhow::Result<Option<String>> {
    if !state.config().steam_enabled() {
        anyhow::bail!("steam/WE feature is disabled");
    }
    let item_directory = state.config().we_dir().join(we_id);
    if !item_directory.is_dir() {
        anyhow::bail!("WE item not found: {}", item_directory.display());
    }
    let (project_type, file) = skwd_wall_core::we::read_project_type(&item_directory);
    if !skwd_wall_core::we::is_supported_project(&project_type) {
        anyhow::bail!("WE item {we_id} has unsupported project type {project_type:?}");
    }
    if project_type == "video" {
        if file.is_empty() {
            anyhow::bail!("WE video item {we_id} has no media file in project.json");
        }
        let Some(video) = skwd_wall_core::we::safe_item_join(&item_directory, &file) else {
            anyhow::bail!("WE item has unsafe video file path: {file}");
        };
        let video = video.display().to_string();
        application.apply_output(ApplyOutputRequest {
            output,
            kind: wall_proto::kind::VIDEO,
            path: &video,
            we_id: "",
            fill_mode: fill,
            mute,
            volume,
            frame_rate: None,
            transition: None,
        })?;
    } else {
        application.apply_output(ApplyOutputRequest {
            output,
            kind: wall_proto::kind::WE,
            path: "",
            we_id,
            fill_mode: fill,
            mute,
            volume,
            frame_rate: None,
            transition: None,
        })?;
    }
    Ok(skwd_wall_core::we::find_preview(&item_directory).map(|path| path.display().to_string()))
}

fn applied_library_key(state: &WallState, kind: &str, path: &str, we_id: &str) -> String {
    if kind == wall_proto::kind::WE && !we_id.is_empty() {
        return format!("we:{we_id}");
    }
    let key = {
        let config = state.config();
        skwd_wall_core::paths::key_for_path(
            std::path::Path::new(path),
            &config.wallpaper_dir(),
            &config.video_dir(),
        )
    };
    key.or_else(|| {
        state
            .with_db(|connection| skwd_wall_core::db::key_for_video_file(connection, path))
            .ok()
            .flatten()
    })
    .unwrap_or_default()
}

pub(crate) fn key_apply_args(
    key: &str,
    wallpaper_dir: &str,
    video_dir: &str,
) -> (&'static str, String, String) {
    if let Some(relative) = key.strip_prefix("static:") {
        (
            wall_proto::kind::STATIC,
            format!("{}/{}", wallpaper_dir.trim_end_matches('/'), relative),
            String::new(),
        )
    } else if let Some(relative) = key.strip_prefix("video:") {
        (
            wall_proto::kind::VIDEO,
            format!("{}/{}", video_dir.trim_end_matches('/'), relative),
            String::new(),
        )
    } else if let Some(id) = key.strip_prefix("we:") {
        (wall_proto::kind::WE, String::new(), id.to_string())
    } else {
        (wall_proto::kind::STATIC, key.to_string(), String::new())
    }
}

pub(crate) fn apply_by_key(
    state: &Arc<WallState>,
    application: &dyn WallpaperApplication,
    history: &dyn HistoryRepository,
    publisher: &dyn EventPublisher,
    stats: &Stats,
    key: &str,
    output: &str,
    notify: bool,
    source: ApplySource,
) -> bool {
    let (kind, path, we_id, mute, volume) = {
        let config = state.config();
        let (kind, path, we_id) = key_apply_args(key, &config.wallpaper_dir(), &config.video_dir());
        let (mute, volume) = skwd_wall_core::audio::resolve_defaults(
            &config.cache_dir(),
            output,
            config.renderer().mute(),
            config.renderer().volume(),
        );
        (kind, path, we_id, mute, volume)
    };
    match apply_core(
        state,
        application,
        history,
        publisher,
        stats,
        kind,
        &path,
        &we_id,
        mute,
        volume,
        source,
        output,
        notify,
        false,
        None,
        None,
    ) {
        Ok(_) => true,
        Err(error) => {
            log::warn!("apply_by_key failed for {key} on {output}: {error}");
            false
        }
    }
}

#[cfg(test)]
#[path = "apply/tests.rs"]
mod tests;
