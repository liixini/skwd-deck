use std::sync::Arc;

use crate::domain::wallpaper::{is_safe_positional, managed_transition_args};
use crate::state::WallState;

use super::engine::apply_static_override;
use super::lifecycle::{record_static, spawn_base_still, validate_source};
use super::policy::record_paper_policy;
use super::resolver::resolve_current_image;
use super::transaction::{ReadyHandoff, ReusePolicy};
use super::transition::TransitionPlan;

const STATIC_TRANSITION_HANDOFF_GRACE: std::time::Duration = std::time::Duration::from_millis(120);

pub fn apply_static_transition(
    state: &WallState,
    from: &str,
    to: &str,
    fill_mode: &str,
    shader: &str,
    duration_ms: u64,
) -> anyhow::Result<()> {
    validate_source(from)?;
    validate_source(to)?;
    if crate::plasma::available() {
        let outputs = crate::outputs::names();
        let transition = crate::infrastructure::paper::TransitionPolicy {
            from: Some(from.to_string()),
            effect: Some(shader.to_string()),
            duration_ms: Some(duration_ms),
        };
        return apply_static_smart_with_outputs(
            state,
            StaticSteadyRequest::new("*", to, fill_mode, &outputs),
            Some(transition),
        );
    }
    if let Some(result) = apply_static_override(state, "*", to, fill_mode) {
        return result;
    }
    let args = managed_transition_args(from, to, fill_mode, shader, duration_ms);
    let overlay = super::RendererLaunchSpec::managed_transition(args).spawn(state)?.wait_ready()?;
    let overlay_pid = overlay.pid();
    let transition_ready_at = std::time::Instant::now();
    let outputs = crate::outputs::names();
    apply_static_smart_with_outputs(
        state,
        StaticSteadyRequest::preserving_transition("*", to, fill_mode, &outputs),
        None,
    )?;
    overlay.commit()?;
    let animation_done_at = transition_ready_at + std::time::Duration::from_millis(duration_ms);
    let retire_at =
        animation_done_at.max(std::time::Instant::now()) + STATIC_TRANSITION_HANDOFF_GRACE;
    let retire_delay = retire_at.saturating_duration_since(std::time::Instant::now());
    state.renderers_shared().allow_session_rendering_for(overlay_pid, retire_delay);
    state.renderers_shared().retire_paper_after(overlay_pid, retire_delay);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_static_per_output_transition(
    state: &WallState,
    outputs: &[String],
    from: &str,
    to: &str,
    fill_mode: &str,
    shader: &str,
    duration_ms: u64,
    transition_primary: Option<&str>,
) -> anyhow::Result<()> {
    validate_source(from)?;
    validate_source(to)?;
    let mut overlays = Vec::with_capacity(outputs.len());
    for output in outputs {
        if transition_primary.is_some_and(|primary| output != primary) {
            continue;
        }
        let output_fill = state.config().display().fill_mode_for(output);
        let args = crate::domain::wallpaper::transition_args_for(
            output,
            from,
            to,
            &output_fill,
            shader,
            duration_ms,
        );
        overlays.push(super::RendererLaunchSpec::standalone_transition(output, args).spawn(state)?);
    }
    let mut ready = Vec::with_capacity(overlays.len());
    for overlay in overlays {
        ready.push(overlay.wait_ready()?);
    }
    let prepared = ready
        .into_iter()
        .map(super::launch::ReadyRenderer::prepare_commit)
        .collect::<anyhow::Result<Vec<_>>>()?;
    for overlay in prepared {
        overlay.finalize();
    }
    apply_static_smart_with_outputs(
        state,
        StaticSteadyRequest::new("*", to, fill_mode, outputs),
        None,
    )
}

enum PaperRetention {
    Retire,
    PreserveTransition,
}

pub(super) struct StaticSteadyRequest<'a> {
    output: &'a str,
    path: &'a str,
    fill_mode: &'a str,
    outputs: &'a [String],
    retention: PaperRetention,
}

impl<'a> StaticSteadyRequest<'a> {
    fn new(output: &'a str, path: &'a str, fill_mode: &'a str, outputs: &'a [String]) -> Self {
        Self { output, path, fill_mode, outputs, retention: PaperRetention::Retire }
    }

    fn preserving_transition(
        output: &'a str,
        path: &'a str,
        fill_mode: &'a str,
        outputs: &'a [String],
    ) -> Self {
        Self { output, path, fill_mode, outputs, retention: PaperRetention::PreserveTransition }
    }
}

pub(super) struct StaticApplyRequest<'a> {
    pub(super) output: &'a str,
    pub(super) path: &'a str,
    pub(super) fill_mode: &'a str,
    pub(super) from: Option<&'a str>,
    pub(super) transition: TransitionPlan,
}

pub(super) fn apply_static_owned(
    state: &Arc<WallState>,
    request: StaticApplyRequest<'_>,
) -> anyhow::Result<()> {
    let StaticApplyRequest { output, path, fill_mode, from, transition } = request;
    let path_owned = resolve_current_image(path);
    let path = path_owned.as_str();
    let from_owned = from.map(resolve_current_image);
    let from = from_owned.as_deref();
    if crate::plasma::available() {
        let outputs = crate::outputs::names();
        let request = StaticSteadyRequest::new(output, path, fill_mode, &outputs);
        let plasma_transition = (transition.enabled()
            && from.is_some_and(|previous| previous != path))
        .then(|| crate::infrastructure::paper::TransitionPolicy {
            from: from.map(str::to_string),
            effect: Some(transition.shader().to_string()),
            duration_ms: Some(transition.duration_ms()),
        });
        return apply_static_smart_with_outputs(state, request, plasma_transition);
    }
    if let Some(result) = apply_static_override(state, output, path, fill_mode) {
        return result;
    }
    debug_assert_eq!(output, "*");
    let outputs = crate::outputs::names();
    let fills: Vec<String> =
        outputs.iter().map(|candidate| state.config().display().fill_mode_for(candidate)).collect();
    let fills_uniform = fills.windows(2).all(|pair| pair[0] == pair[1]);
    let fill_mode =
        if fills_uniform { fills.first().map_or(fill_mode, String::as_str) } else { fill_mode };
    let want_transition = transition.enabled() && from.is_some_and(|previous| previous != path);
    if let Some(from) = from.filter(|_| want_transition) {
        let transition_primary =
            super::transition::transition_primary(state, &outputs, transition.shader());
        if fills_uniform && transition_primary.is_none() {
            apply_static_transition(
                state,
                from,
                path,
                fill_mode,
                transition.shader(),
                transition.duration_ms(),
            )?;
        } else {
            log::info!(
                "apply: per-output fill modes differ, using {} synchronized overlays",
                outputs.len()
            );
            apply_static_per_output_transition(
                state,
                &outputs,
                from,
                path,
                fill_mode,
                transition.shader(),
                transition.duration_ms(),
                transition_primary.as_deref(),
            )?;
        }
    } else {
        apply_static_smart_with_outputs(
            state,
            StaticSteadyRequest::new("*", path, fill_mode, &outputs),
            None,
        )?;
    }
    Ok(())
}

pub fn apply_static_smart(
    state: &WallState,
    output: &str,
    path: &str,
    fill_mode: &str,
) -> anyhow::Result<()> {
    let outputs = crate::outputs::names();
    apply_static_smart_with_outputs(
        state,
        StaticSteadyRequest::new(output, path, fill_mode, &outputs),
        None,
    )
}

pub(super) fn apply_static_smart_with_outputs(
    state: &WallState,
    request: StaticSteadyRequest<'_>,
    plasma_transition: Option<crate::infrastructure::paper::TransitionPolicy>,
) -> anyhow::Result<()> {
    let StaticSteadyRequest { output, path, fill_mode, outputs, retention } = request;
    let path_owned = resolve_current_image(path);
    let path = path_owned.as_str();
    validate_source(path)?;
    if !crate::plasma::available()
        && let Some(result) = apply_static_override(state, output, path, fill_mode)
    {
        return result;
    }
    let previous_assignments = state.renderers().assignments();
    if output == "*" {
        state.renderers().set_all_assignments(outputs, path);
    } else {
        state.renderers().set_assignment(output, path);
    }
    let assignments = state.renderers().assignments();
    let resolved = resolve_assignments(outputs, &assignments, path);
    if crate::plasma::available() {
        record_static(state, &resolved);
        crate::plasma::apply_current_with_transition(state, output, plasma_transition)?;
        crate::plasma::retire_native(state);
        record_paper_policy(state);
        return Ok(());
    }
    let fills: Vec<String> = resolved
        .iter()
        .map(|(candidate, _)| state.config().display().fill_mode_for(candidate))
        .collect();
    let uniform = resolved.iter().all(|(_, assigned)| *assigned == resolved[0].1)
        && fills.iter().all(|fill| *fill == fills[0]);
    if let Err(error) =
        spawn_resolved_stills(state, outputs, &resolved, &previous_assignments, uniform, fill_mode)
    {
        state.renderers().replace_assignments(previous_assignments);
        return Err(error);
    }
    if matches!(retention, PaperRetention::Retire) {
        state.renderers().kill_paper();
    }
    state.renderers().kill_video_papers();
    state.renderers().kill_holders();
    record_paper_policy(state);
    record_static(state, &resolved);
    Ok(())
}

fn resolve_assignments(
    outputs: &[String],
    assignments: &std::collections::HashMap<String, String>,
    path: &str,
) -> Vec<(String, String)> {
    if outputs.is_empty() {
        return vec![(String::from("*"), path.to_string())];
    }
    outputs
        .iter()
        .map(|output| {
            let assigned = assignments.get(output).cloned().unwrap_or_else(|| path.to_string());
            (output.clone(), assigned)
        })
        .collect()
}

fn spawn_resolved_stills(
    state: &WallState,
    outputs: &[String],
    resolved: &[(String, String)],
    previous_assignments: &std::collections::HashMap<String, String>,
    uniform: bool,
    fill_mode: &str,
) -> anyhow::Result<()> {
    if uniform {
        let path = resolved[0].1.clone();
        let fill = resolved.first().map_or_else(
            || fill_mode.to_string(),
            |(output, _)| state.config().display().fill_mode_for(output),
        );
        if !state.renderers().still_swap(&path, &fill) {
            spawn_base_still(state, "*", &path, &fill)?.wait_ready()?.commit()?;
        }
        state.renderers().kill_output_stills();
        return Ok(());
    }
    let mut swapped = Vec::new();
    let mut all_swapped = true;
    for (output, path) in resolved {
        let fill = state.config().display().fill_mode_for(output);
        if state.renderers().output_still_swap(output, path, &fill) {
            swapped.push(output.clone());
        } else {
            all_swapped = false;
            break;
        }
    }
    if all_swapped {
        state.renderers().retain_output_stills(outputs);
        state.renderers().kill_base_still();
        return Ok(());
    }
    for output in swapped {
        if let Some(previous) = previous_assignments.get(&output) {
            let fill = state.config().display().fill_mode_for(&output);
            let _ = state.renderers().output_still_swap(&output, previous, &fill);
        }
    }
    let mut pending = Vec::new();
    for (output, path) in resolved {
        let fill = state.config().display().fill_mode_for(output);
        pending.push(spawn_base_still(state, output, path, &fill)?);
    }
    let mut ready = Vec::with_capacity(pending.len());
    for renderer in pending {
        ready.push(renderer.wait_ready()?);
    }
    let prepared = ready
        .into_iter()
        .map(super::launch::ReadyRenderer::prepare_commit)
        .collect::<anyhow::Result<Vec<_>>>()?;
    for renderer in prepared {
        renderer.finalize();
    }
    state.renderers().retain_output_stills(outputs);
    state.renderers().kill_base_still();
    Ok(())
}

pub(super) struct StaticMultiRequest<'a, 'state> {
    pub(super) map: &'a serde_json::Map<String, serde_json::Value>,
    pub(super) targets: &'a [String],
    pub(super) keep_still: &'a mut Vec<String>,
    pub(super) pending: &'a mut Vec<ReadyHandoff<'state>>,
    pub(super) reuse: ReusePolicy,
}

pub(super) fn reconcile_static_multi<'a>(
    state: &'a WallState,
    request: StaticMultiRequest<'_, 'a>,
) -> std::collections::HashSet<String> {
    let StaticMultiRequest { map, targets, keep_still, pending, reuse } = request;
    let mut groups: std::collections::BTreeMap<(String, String), Vec<String>> =
        std::collections::BTreeMap::new();
    for output in targets {
        let Some(entry) = map.get(output) else { continue };
        if entry.get("type").and_then(serde_json::Value::as_str) != Some(wall_proto::kind::STATIC) {
            continue;
        }
        let path = entry.get("path").and_then(serde_json::Value::as_str).unwrap_or("");
        if path.is_empty() || !is_safe_positional(path) || output.contains(',') {
            continue;
        }
        groups
            .entry((path.to_string(), state.config().display().fill_mode_for(output)))
            .or_default()
            .push(output.clone());
    }
    let mut handled = std::collections::HashSet::new();
    for ((path, fill), mut outputs) in groups {
        if outputs.len() < 2 {
            continue;
        }
        outputs.sort();
        let key = outputs.join(",");
        let reused = reuse.allows_warm() && state.renderers().output_still_swap(&key, &path, &fill);
        if reused {
            for output in &outputs {
                state.renderers().set_assignment(output, &path);
                state.renderers().kill_output_still(output);
            }
        } else {
            let renderer = match spawn_base_still(state, &key, &path, &fill) {
                Ok(renderer) => renderer,
                Err(error) => {
                    log::warn!("shared still for {key} failed to spawn ({error:#}), per-output");
                    continue;
                }
            };
            match renderer.wait_ready() {
                Ok(renderer) => {
                    pending.push(super::transaction::ReadyHandoff {
                        renderer,
                        assignments: outputs
                            .iter()
                            .map(|output| (output.clone(), path.clone()))
                            .collect(),
                        transition_duration: None,
                    });
                }
                Err(error) => {
                    log::warn!("shared still for {key} failed readiness ({error:#}), per-output");
                    continue;
                }
            }
        }
        log::info!("reconcile: {} outputs share one still renderer ({key})", outputs.len());
        keep_still.push(key);
        handled.extend(outputs);
    }
    handled
}

#[derive(Clone, Copy)]
pub(super) struct StaticReconcileRequest<'a> {
    pub(super) output: &'a str,
    pub(super) path: &'a str,
    pub(super) previous: &'a str,
    pub(super) reuse: ReusePolicy,
}

pub(super) fn reconcile_static<'a>(
    state: &'a WallState,
    request: StaticReconcileRequest<'_>,
) -> anyhow::Result<Option<ReadyHandoff<'a>>> {
    let StaticReconcileRequest { output, path, previous, reuse } = request;
    if previous == path && state.renderers().has_output_still(output) {
        log::info!("reconcile {output}: static {path} (unchanged, keep)");
        return Ok(None);
    }
    let output_fill = state.config().display().fill_mode_for(output);
    let slide = reuse.allows_warm().then(|| state.apply().take_swap_slide()).flatten();
    let reused = match &slide {
        Some((direction, slide_duration)) if reuse.allows_warm() => state
            .renderers()
            .output_still_swap_slide(output, path, direction, *slide_duration, &output_fill),
        _ if reuse.allows_warm() => state.renderers().output_still_swap(output, path, &output_fill),
        _ => false,
    };
    let handoff = if reused {
        state.renderers().kill_video_paper(output);
        None
    } else {
        let renderer = spawn_base_still(state, output, path, &output_fill)?.wait_ready()?;
        Some(super::transaction::ReadyHandoff {
            renderer,
            assignments: vec![(output.to_string(), path.to_string())],
            transition_duration: None,
        })
    };
    if handoff.is_none() {
        state.renderers().set_assignment(output, path);
    }
    let mode = match (reused, slide.is_some()) {
        (true, true) => "slide",
        (true, false) => "swap",
        _ => "spawn",
    };
    log::info!("reconcile {output}: static {path} ({mode})");
    Ok(handoff)
}
