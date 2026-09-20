use crate::domain::wallpaper::managed_transition_args;
use crate::state::WallState;

use super::super::engine::apply_static_override;
use super::super::lifecycle::{record_static, validate_source};
use super::super::policy::record_paper_policy;
use super::{StaticSteadyRequest, apply_static_smart_with_outputs, resolve_assignments};

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
            fps: None,
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
    let still = super::super::RendererLaunchSpec::static_for("*", to, fill_mode)
        .prepare_hidden(true)
        .spawn(state)?
        .wait_ready()?;
    let mut args = managed_transition_args(from, to, fill_mode, shader, duration_ms);
    args.push("--transition-hold".to_string());
    let overlay =
        super::super::RendererLaunchSpec::managed_transition(args).spawn(state)?.wait_ready()?;
    let still = still.reveal_still()?.prepare_commit()?;
    let mut overlay = overlay.prepare_commit()?;
    overlay.start_transition()?;
    let overlay_pid = overlay.finalize();
    still.finalize();
    state.renderers().kill_output_stills();
    state.renderers().kill_video_papers();
    state.renderers().kill_holders();
    let outputs = crate::outputs::names();
    state.renderers().set_all_assignments(&outputs, to);
    record_static(state, &resolve_assignments(&outputs, &state.renderers().assignments(), to));
    record_paper_policy(state);
    let retire_delay =
        std::time::Duration::from_millis(duration_ms) + STATIC_TRANSITION_HANDOFF_GRACE;
    state.renderers_shared().allow_session_rendering_for(overlay_pid, retire_delay);
    state.renderers_shared().retire_paper_after(overlay_pid, retire_delay);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_static_per_output_transition(
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
    let mut groups = std::collections::BTreeMap::<String, Vec<String>>::new();
    for output in outputs {
        groups
            .entry(state.config().display().fill_mode_for(output))
            .or_default()
            .push(output.clone());
    }
    if groups.is_empty() {
        groups.insert(fill_mode.to_string(), vec!["*".to_string()]);
    }
    let shared = groups.len() == 1;
    let mut stills = Vec::new();
    let mut keep = Vec::new();
    let mut plans = Vec::new();
    for (fill, group) in groups {
        let key = if shared { "*".to_string() } else { group.join(",") };
        stills.push(
            super::super::RendererLaunchSpec::static_for(&key, to, &fill)
                .prepare_hidden(true)
                .spawn(state)?
                .wait_ready()?,
        );
        keep.push(key.clone());
        let target =
            transition_primary.filter(|primary| group.iter().any(|output| output == primary));
        if transition_primary.is_none() || target.is_some() {
            let target = target.unwrap_or(&key);
            let args = crate::domain::wallpaper::transition_args_for(
                target,
                from,
                to,
                &fill,
                shader,
                duration_ms,
            );
            plans.push((target.to_string(), args));
        }
    }
    let mut overlays = Vec::new();
    for (target, args) in plans {
        overlays.push(
            super::super::RendererLaunchSpec::staged_transition(&target, args)
                .spawn(state)?
                .wait_ready()?,
        );
    }
    let stills = stills
        .into_iter()
        .map(|renderer| renderer.reveal_still()?.prepare_commit())
        .collect::<anyhow::Result<Vec<_>>>()?;
    let mut overlays = overlays
        .into_iter()
        .map(super::super::launch::ReadyRenderer::prepare_commit)
        .collect::<anyhow::Result<Vec<_>>>()?;
    for overlay in &mut overlays {
        overlay.start_transition()?;
    }
    for still in stills {
        still.finalize();
    }
    for overlay in overlays {
        overlay.finalize();
    }
    if shared {
        state.renderers().kill_output_stills();
    } else {
        state.renderers().kill_base_still();
        state.renderers().retain_output_stills(&keep);
    }
    state.renderers().kill_paper();
    state.renderers().kill_video_papers();
    state.renderers().kill_holders();
    state.renderers().set_all_assignments(outputs, to);
    record_static(state, &resolve_assignments(outputs, &state.renderers().assignments(), to));
    record_paper_policy(state);
    Ok(())
}
