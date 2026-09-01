use crate::domain::wallpaper::{is_safe_positional, video_transition_args, vk_video_args};
use crate::state::WallState;

use super::launch::READY_TIMEOUT;
use super::lifecycle::{
    allow_transition_to_finish, defer_kill_after_swap, record_and_dedup, set_all_video_assignments,
    spawn_video_paper, validate_source,
};
use super::policy::{paper_policy_matches, record_paper_policy};
use super::resolver::resolve_current_video;
use super::transaction::{ReadyHandoff, ReusePolicy};
use super::transition::TransitionPlan;

#[allow(clippy::too_many_arguments)]
pub fn apply_video_transition(
    state: &WallState,
    from: &str,
    to: &str,
    fill_mode: &str,
    shader: &str,
    duration_ms: u64,
    mute: bool,
    volume: u32,
) -> anyhow::Result<()> {
    let from_owned = resolve_current_video(from);
    let from = from_owned.as_str();
    let to_owned = resolve_current_video(to);
    let to = to_owned.as_str();
    validate_source(from)?;
    validate_source(to)?;
    crate::awww::stop();
    if crate::plasma::available() {
        let outputs = crate::outputs::names();
        set_all_video_assignments(state, &outputs, to);
        record_and_dedup(state, &outputs, wall_proto::kind::VIDEO, to, "", mute, volume);
        crate::plasma::apply_current_with_transition(
            state,
            "*",
            Some(crate::infrastructure::paper::TransitionPolicy {
                from: Some(from.to_string()),
                effect: Some(shader.to_string()),
                duration_ms: Some(duration_ms),
            }),
        )?;
        crate::plasma::retire_native(state);
        record_paper_policy(state);
        return Ok(());
    }
    let plan =
        super::transition::TransitionSelection::Explicit { enabled: true, shader, duration_ms }
            .resolve(state);
    if paper_policy_matches(state)
        && crate::domain::wallpaper::is_video_path(from)
        && state.renderers().has_video_paper("*")
        && !state.renderers().is_scene_paper("*")
    {
        let swap_pid = state.renderers().video_paper_pid("*");
        if let Some(pid) = swap_pid {
            state.renderers().arm_ready_gate(pid);
        }
        if state.renderers().video_swap_fade(
            "*",
            to,
            plan.shader(),
            plan.duration_ms(),
            mute,
            volume,
        ) {
            if !swap_pid.is_some_and(|pid| state.renderers().wait_ready(pid, READY_TIMEOUT)) {
                anyhow::bail!(
                    "Vulkan video renderer rejected or timed out during transition warm swap"
                );
            }
            if let Some(pid) = swap_pid {
                allow_transition_to_finish(state, pid, plan.duration_ms());
            }
            state.renderers().defer_kill_stills(800);
            state.renderers().kill_holders();
            record_and_dedup(
                state,
                &["*".to_string()],
                wall_proto::kind::VIDEO,
                to,
                "",
                mute,
                volume,
            );
            record_paper_policy(state);
            return Ok(());
        }
    }
    if !from.is_empty() && from != to {
        let outputs = crate::outputs::names();
        let previous_assignments = state.renderers().assignments();
        set_all_video_assignments(state, &outputs, to);
        let result = apply_video_vk(
            state,
            to,
            fill_mode,
            mute,
            volume,
            VideoPlayback::Transition { from, plan: &plan },
            StateRecording::Persist,
        );
        if result.is_ok() {
            state.renderers().kill_holders();
        } else {
            state.renderers().replace_assignments(previous_assignments);
        }
        return result;
    }
    apply_video(state, "*", to, fill_mode, mute, volume)
}

pub fn apply_video(
    state: &WallState,
    output: &str,
    path: &str,
    fill_mode: &str,
    mute: bool,
    volume: u32,
) -> anyhow::Result<()> {
    apply_video_request(
        state,
        VideoApplyRequest {
            output,
            path,
            fill_mode,
            mute,
            volume,
            recording: StateRecording::Persist,
        },
    )
}

#[derive(Clone, Copy)]
pub(super) enum StateRecording {
    Persist,
    PreserveExisting,
}

#[derive(Clone, Copy)]
pub(super) struct VideoApplyRequest<'a> {
    pub(super) output: &'a str,
    pub(super) path: &'a str,
    pub(super) fill_mode: &'a str,
    pub(super) mute: bool,
    pub(super) volume: u32,
    pub(super) recording: StateRecording,
}

pub(super) fn apply_video_request(
    state: &WallState,
    request: VideoApplyRequest<'_>,
) -> anyhow::Result<()> {
    let VideoApplyRequest { output, path, fill_mode, mute, volume, recording } = request;
    let path_owned = resolve_current_video(path);
    let path = path_owned.as_str();
    validate_source(path)?;
    crate::awww::stop();
    let outputs = crate::outputs::names();
    let previous_assignments = state.renderers().assignments();
    set_all_video_assignments(state, &outputs, path);
    if crate::plasma::available() {
        if matches!(recording, StateRecording::Persist) {
            record_and_dedup(state, &outputs, wall_proto::kind::VIDEO, path, "", mute, volume);
        }
        let result = crate::plasma::apply_current(state);
        if result.is_ok() {
            crate::plasma::retire_native(state);
            record_paper_policy(state);
        } else {
            state.renderers().replace_assignments(previous_assignments);
        }
        return result;
    }
    let _ = output;
    let result =
        apply_video_vk(state, path, fill_mode, mute, volume, VideoPlayback::Steady, recording);
    if result.is_ok() {
        state.renderers().kill_holders();
    } else {
        state.renderers().replace_assignments(previous_assignments);
    }
    result
}

pub(super) fn apply_video_vk(
    state: &WallState,
    path: &str,
    fill_mode: &str,
    mute: bool,
    volume: u32,
    playback: VideoPlayback<'_>,
    recording: StateRecording,
) -> anyhow::Result<()> {
    let transition_duration_ms = playback.duration_ms();
    let swap_pid = state.renderers().video_paper_pid("*");
    if let Some(pid) = swap_pid {
        state.renderers().arm_ready_gate(pid);
    }
    if paper_policy_matches(state)
        && state.renderers().has_video_paper("*")
        && !state.renderers().is_scene_paper("*")
        && state.renderers().video_swap("*", path, mute, volume)
    {
        if !swap_pid.is_some_and(|pid| state.renderers().wait_ready(pid, READY_TIMEOUT)) {
            anyhow::bail!("Vulkan video renderer rejected or timed out during warm swap");
        }
        if let (Some(pid), Some(duration_ms)) = (swap_pid, transition_duration_ms) {
            allow_transition_to_finish(state, pid, duration_ms);
        }
        let old = state.renderers().take_video_papers_except(&["*".to_string()]);
        defer_kill_after_swap(None, old, state.renderers().take_paper());
        state.renderers().defer_kill_stills(800);
        if matches!(recording, StateRecording::Persist) {
            record_and_dedup(
                state,
                &["*".to_string()],
                wall_proto::kind::VIDEO,
                path,
                "",
                mute,
                volume,
            );
        }
        record_paper_policy(state);
        return Ok(());
    }
    let args = match playback {
        VideoPlayback::Transition { from, plan } => {
            crate::domain::wallpaper::video_transition_args(
                "*",
                from,
                path,
                fill_mode,
                plan.shader(),
                plan.duration_ms(),
                mute,
                volume,
            )
        }
        VideoPlayback::Steady => vk_video_args("*", path, fill_mode, mute, volume),
    };
    let startup = spawn_video_paper(state, "*", &args)?;
    let restored = startup.has_displaced();
    let ready = match startup.wait_ready() {
        Ok(ready) => ready,
        Err(_) if restored => {
            anyhow::bail!("Vulkan renderer did not become ready; restored previous renderer");
        }
        Err(error) => return Err(error),
    };
    let pid = ready.pid();
    if let Some(duration_ms) = transition_duration_ms {
        allow_transition_to_finish(state, pid, duration_ms);
    }
    ready.commit()?;
    state.renderers().defer_kill_stills(800);
    if matches!(recording, StateRecording::Persist) {
        record_and_dedup(
            state,
            &["*".to_string()],
            wall_proto::kind::VIDEO,
            path,
            "",
            mute,
            volume,
        );
    }
    record_paper_policy(state);
    Ok(())
}

#[derive(Clone, Copy)]
pub(super) enum VideoPlayback<'a> {
    Steady,
    Transition { from: &'a str, plan: &'a super::transition::TransitionPlan },
}

impl VideoPlayback<'_> {
    fn duration_ms(&self) -> Option<u64> {
        match self {
            Self::Steady => None,
            Self::Transition { plan, .. } => Some(plan.duration_ms()),
        }
    }
}

const MULTI_KEY: &str = "multi";

pub(super) struct VideoMultiRequest<'a, 'state> {
    pub(super) map: &'a serde_json::Map<String, serde_json::Value>,
    pub(super) targets: &'a [String],
    pub(super) previous_assignments: &'a std::collections::HashMap<String, String>,
    pub(super) keep_video: &'a mut Vec<String>,
    pub(super) pending: &'a mut Vec<ReadyHandoff<'state>>,
    pub(super) transition: Option<&'a TransitionPlan>,
    pub(super) transition_primary: Option<&'a str>,
}

pub(super) fn reconcile_video_multi<'a>(
    state: &'a WallState,
    request: VideoMultiRequest<'_, 'a>,
) -> bool {
    let VideoMultiRequest {
        map,
        targets,
        previous_assignments,
        keep_video,
        pending,
        transition,
        transition_primary,
    } = request;
    if !(state.config().renderer().video_multi_process()
        || state.config().renderer().performance_mode())
    {
        return false;
    }
    if targets
        .iter()
        .filter(|output| {
            map.get(*output).and_then(|entry| entry.get("type")).and_then(serde_json::Value::as_str)
                == Some(wall_proto::kind::VIDEO)
        })
        .take(2)
        .count()
        < 2
    {
        return false;
    }
    let mut pairs: Vec<(String, String)> = Vec::new();
    let mut entries: Vec<paper_control::MultiVideoEntry> = Vec::new();
    let mut fills: Vec<String> = Vec::new();
    for output in targets {
        let Some(entry) = map.get(output) else { continue };
        if entry.get("type").and_then(serde_json::Value::as_str) != Some(wall_proto::kind::VIDEO) {
            continue;
        }
        let path = entry.get("path").and_then(serde_json::Value::as_str).unwrap_or("");
        if path.is_empty()
            || !is_safe_positional(path)
            || output.contains(',')
            || output.contains('=')
        {
            return false;
        }
        pairs.push((output.clone(), path.to_string()));
        let transition_from = transition
            .is_some_and(|_| {
                super::transition::transitions_for_output(true, transition_primary, output)
            })
            .then(|| previous_assignments.get(output).map_or("", String::as_str))
            .filter(|previous| !previous.is_empty() && *previous != path)
            .and_then(|previous| super::transition::previous_media_source(state, output, previous));
        entries.push(paper_control::MultiVideoEntry {
            output: output.clone(),
            video: path.to_string(),
            mute: entry.get("mute").and_then(serde_json::Value::as_bool).unwrap_or(true),
            volume: entry
                .get("volume")
                .and_then(serde_json::Value::as_u64)
                .map_or(100, |volume| (volume as u32).min(100)),
            transition_from,
        });
        fills.push(state.config().display().fill_mode_for(output));
    }
    if pairs.len() < 2 {
        return false;
    }
    fills.sort();
    fills.dedup();
    if fills.len() > 1 {
        log::info!("multi video wall: mixed fill modes, using per-output renderers");
        return false;
    }
    pairs.sort();
    let specification =
        pairs.iter().map(|(output, path)| format!("{output}={path}")).collect::<Vec<_>>().join(";");
    if paper_policy_matches(state)
        && previous_assignments.get(MULTI_KEY) == Some(&specification)
        && state.renderers().has_video_paper(MULTI_KEY)
    {
        log::info!("multi video wall: unchanged, keep");
        keep_video.push(MULTI_KEY.to_string());
        return true;
    }
    entries.sort_by(|left, right| left.output.cmp(&right.output));
    let manifest = match serde_json::to_string(&entries) {
        Ok(manifest) => manifest,
        Err(error) => {
            log::warn!("multi video wall: manifest failed ({error}), using per-output renderers");
            return false;
        }
    };
    let mut arguments = vec![
        "--multi-json".to_string(),
        manifest,
        "--fill-mode".to_string(),
        fills.first().cloned().unwrap_or_default(),
    ];
    let transition =
        transition.filter(|_| entries.iter().any(|entry| entry.transition_from.is_some()));
    if let Some(plan) = transition {
        arguments.extend([
            "--shader".to_string(),
            plan.shader().to_string(),
            "--duration-ms".to_string(),
            plan.duration_ms().to_string(),
        ]);
    }
    match spawn_video_paper(state, MULTI_KEY, &arguments) {
        Ok(startup) => {
            if let Ok(renderer) = startup.wait_ready() {
                let mut assignments = pairs.clone();
                assignments.push((MULTI_KEY.to_string(), specification));
                pending.push(super::transaction::ReadyHandoff {
                    renderer,
                    assignments,
                    transition_duration: transition.map(TransitionPlan::duration_ms),
                });
                keep_video.push(MULTI_KEY.to_string());
                log::info!("multi video wall: single process up ({} outputs)", pairs.len());
                true
            } else {
                log::warn!(
                    "multi video wall: renderer never signaled ready, using per-output renderers"
                );
                false
            }
        }
        Err(error) => {
            log::warn!("multi video wall: spawn failed ({error:#}), using per-output renderers");
            false
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct VideoReconcileRequest<'a> {
    pub(super) output: &'a str,
    pub(super) path: &'a str,
    pub(super) previous: &'a str,
    pub(super) transition: Option<&'a TransitionPlan>,
    pub(super) mute: bool,
    pub(super) volume: u32,
    pub(super) reuse: ReusePolicy,
}

pub(super) fn reconcile_video<'a>(
    state: &'a WallState,
    request: VideoReconcileRequest<'_>,
) -> anyhow::Result<Option<ReadyHandoff<'a>>> {
    let VideoReconcileRequest { output, path, previous, transition, mute, volume, reuse } = request;
    if previous == path && paper_policy_matches(state) && state.renderers().has_video_paper(output)
    {
        log::info!("reconcile {output}: video {path} (unchanged, keep)");
        return Ok(None);
    }
    let handoff = reconcile_video_vk(
        state,
        VideoReconcileRequest { output, path, previous, transition, mute, volume, reuse },
    )?;
    if handoff.is_none() {
        state.renderers().kill_output_still(output);
        state.renderers().set_assignment(output, path);
    }
    Ok(handoff)
}

fn reconcile_video_vk<'a>(
    state: &'a WallState,
    request: VideoReconcileRequest<'_>,
) -> anyhow::Result<Option<ReadyHandoff<'a>>> {
    let VideoReconcileRequest { output, path, previous, transition, mute, volume, reuse } = request;
    let transitioning = transition.is_some() && !previous.is_empty() && previous != path;
    let swapped = reuse.allows_warm()
        && paper_policy_matches(state)
        && state.renderers().has_video_paper(output)
        && !state.renderers().is_scene_paper(output)
        && if transitioning {
            let plan = transition.expect("transitioning has plan");
            state.renderers().video_swap_fade(
                output,
                path,
                plan.shader(),
                plan.duration_ms(),
                mute,
                volume,
            )
        } else {
            state.renderers().video_swap(output, path, mute, volume)
        };
    if swapped {
        if transitioning && let Some(pid) = state.renderers().video_paper_pid(output) {
            allow_transition_to_finish(
                state,
                pid,
                transition.expect("transitioning has plan").duration_ms(),
            );
        }
        log::info!("reconcile {output}: video {path} (vk swap)");
        return Ok(None);
    }
    let fill = state.config().display().fill_mode_for(output);
    let from = super::transition::previous_media_source(state, output, previous);
    let (arguments, mode, transitioning) = match from.filter(|from| from != path) {
        Some(from) if transition.is_some() => (
            video_transition_args(
                output,
                &from,
                path,
                &fill,
                transition.expect("matched transition").shader(),
                transition.expect("matched transition").duration_ms(),
                mute,
                volume,
            ),
            "vk transition",
            true,
        ),
        _ => (vk_video_args(output, path, &fill, mute, volume), "vk ready handoff", false),
    };
    let renderer = spawn_video_paper(state, output, &arguments)?.wait_ready()?;
    log::info!("reconcile {output}: video {path} ({mode})");
    Ok(Some(super::transaction::ReadyHandoff {
        renderer,
        assignments: vec![(output.to_string(), path.to_string())],
        transition_duration: transitioning
            .then(|| transition.expect("transitioning has plan").duration_ms()),
    }))
}
