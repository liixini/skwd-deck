use std::collections::BTreeMap;

use crate::domain::wallpaper::is_safe_positional;
use crate::state::WallState;

use super::static_media::{
    StaticMultiRequest, StaticReconcileRequest, reconcile_static, reconcile_static_multi,
};
use super::transaction::{PreparedHandoff, ReadyHandoff, ReusePolicy};
use super::transition::{
    TransitionPlan, static_overlay_plans, transition_primary, transitions_for_output,
};
use super::video_media::{
    VideoMultiRequest, VideoReconcileRequest, reconcile_video, reconcile_video_multi,
};
use super::wallpaper_engine::{CommitReadyWe, PreparedWe, group_output, prepare_we};
use super::{PreparedRenderer, ReadyRenderer, RendererLaunchSpec};

struct PreparedBatch<'a> {
    handoffs: Vec<PreparedHandoff<'a>>,
    we: CommitReadyWe<'a>,
    overlays: Vec<PreparedRenderer<'a>>,
}

impl PreparedBatch<'_> {
    fn start_overlays(&mut self) -> anyhow::Result<()> {
        for overlay in &mut self.overlays {
            overlay.start_transition()?;
        }
        Ok(())
    }

    fn finalize(self, state: &WallState) {
        for overlay in self.overlays {
            overlay.finalize();
        }
        for handoff in self.handoffs {
            handoff.finalize(state);
        }
        self.we.finalize(state);
    }
}

fn prepare_batch<'a>(
    handoffs: Vec<ReadyHandoff<'a>>,
    we: PreparedWe<'a>,
    overlays: Vec<ReadyRenderer<'a>>,
) -> anyhow::Result<PreparedBatch<'a>> {
    let handoffs = handoffs
        .into_iter()
        .map(ReadyHandoff::prepare_commit)
        .collect::<anyhow::Result<Vec<_>>>()?;
    let we = we.prepare_commit()?;
    let overlays = overlays
        .into_iter()
        .map(ReadyRenderer::prepare_commit)
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(PreparedBatch { handoffs, we, overlays })
}

pub(super) fn reconcile_outputs(
    state: &WallState,
    monitors: &[String],
    intent: &ReconcileIntent,
) -> anyhow::Result<()> {
    reconcile_outputs_inner(state, monitors, intent)
}

pub(super) enum ReconcileIntent {
    Apply { transition: TransitionPlan },
    PolicyRefresh,
}

impl ReconcileIntent {
    fn transition(&self) -> Option<&TransitionPlan> {
        match self {
            Self::Apply { transition } => Some(transition),
            Self::PolicyRefresh => None,
        }
    }

    fn updates_audio_state(&self) -> bool {
        matches!(self, Self::Apply { .. })
    }
}

#[allow(clippy::too_many_lines)]
fn reconcile_outputs_inner(
    state: &WallState,
    monitors: &[String],
    intent: &ReconcileIntent,
) -> anyhow::Result<()> {
    let transition = intent.transition();
    let transition = transition.filter(|plan| plan.enabled());
    let cache = state.config().cache_dir();
    let current = crate::audio::read_state(&cache);
    let map = match current.as_object() {
        Some(object) => object.clone(),
        None => return Ok(()),
    };
    let output_info = crate::outputs::enumerate();
    let previous_assignments = state.renderers().assignments();
    if crate::plasma::available() {
        let targets = super::resolver::reconcile_targets(monitors, &map);
        let primary =
            transition.and_then(|plan| transition_primary(state, &targets, plan.shader()));
        let transitions = output_info
            .iter()
            .filter_map(|output| {
                let plan = transition?;
                let entry = map.get(&output.name)?;
                let path = entry.get("path").and_then(serde_json::Value::as_str).unwrap_or("");
                let previous = previous_assignments.get(&output.name).map_or("", String::as_str);
                let from = transitions_for_output(true, primary.as_deref(), &output.name)
                    .then(|| {
                        super::transition::previous_media_source(state, &output.name, previous)
                    })
                    .flatten()
                    .filter(|from| from != path)?;
                Some((
                    output.name.clone(),
                    crate::infrastructure::paper::TransitionPolicy {
                        from: Some(from),
                        effect: Some(plan.shader().to_string()),
                        duration_ms: Some(plan.duration_ms()),
                    },
                ))
            })
            .collect();
        crate::plasma::apply(state, &output_info, &map, &transitions)?;
        let mut applied = 0;
        for output in &output_info {
            if let Some(entry) = map.get(&output.name) {
                let kind = entry.get("type").and_then(serde_json::Value::as_str).unwrap_or("");
                let path = entry.get("path").and_then(serde_json::Value::as_str).unwrap_or("");
                let we_id = entry.get("we_id").and_then(serde_json::Value::as_str).unwrap_or("");
                let assigned =
                    if kind == wall_proto::kind::WE && path.is_empty() { we_id } else { path };
                state.renderers().set_assignment(&output.name, assigned);
                applied += 1;
            }
        }
        crate::plasma::retire_native(state);
        if intent.updates_audio_state() {
            crate::audio::mute_dedup_losers(state, &cache);
        }
        log::info!("reconcile: applied {applied} output(s) through Plasma wallpaper plugin");
        return Ok(());
    }
    let mut keep_video: Vec<String> = Vec::new();
    let mut keep_still: Vec<String> = Vec::new();
    let mut we_groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut pending: Vec<ReadyHandoff> = Vec::new();
    let targets = super::resolver::reconcile_targets(monitors, &map);
    let desired_targets = targets
        .iter()
        .filter(|output| {
            map.get(*output)
                .and_then(|entry| entry.get("type"))
                .and_then(serde_json::Value::as_str)
                .is_some_and(|kind| {
                    matches!(
                        kind,
                        wall_proto::kind::STATIC | wall_proto::kind::VIDEO | wall_proto::kind::WE
                    )
                })
        })
        .count();
    let reuse = if desired_targets <= 1 { ReusePolicy::WarmAllowed } else { ReusePolicy::ColdOnly };
    let transition_primary =
        transition.and_then(|plan| transition_primary(state, &targets, plan.shader()));
    warn_unlisted_targets(monitors, &targets);

    let overlay_plans = static_overlay_plans(
        state,
        &map,
        &targets,
        &previous_assignments,
        transition,
        transition_primary.as_deref(),
    );
    let overlay_startups = overlay_plans
        .into_iter()
        .map(|plan| RendererLaunchSpec::staged_transition(&plan.output, plan.args).spawn(state))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let mut overlays = Vec::with_capacity(overlay_startups.len());
    for overlay in overlay_startups {
        overlays.push(overlay.wait_ready()?);
    }

    let static_handled = reconcile_static_multi(
        state,
        StaticMultiRequest {
            map: &map,
            targets: &targets,
            keep_still: &mut keep_still,
            pending: &mut pending,
            reuse,
        },
    );
    let multi_video_handled = reconcile_video_multi(
        state,
        VideoMultiRequest {
            map: &map,
            targets: &targets,
            previous_assignments: &previous_assignments,
            keep_video: &mut keep_video,
            pending: &mut pending,
            transition,
            transition_primary: transition_primary.as_deref(),
        },
    );

    for output in &targets {
        let Some(entry) = map.get(output) else {
            log::info!("reconcile {output}: no desired entry, leaving as-is");
            continue;
        };
        let kind = entry.get("type").and_then(serde_json::Value::as_str).unwrap_or("");
        let path = entry.get("path").and_then(serde_json::Value::as_str).unwrap_or("");
        let we_id = entry.get("we_id").and_then(serde_json::Value::as_str).unwrap_or("");
        let mute = entry.get("mute").and_then(serde_json::Value::as_bool).unwrap_or(true);
        let volume = entry.get("volume").and_then(serde_json::Value::as_u64).unwrap_or(100) as u32;
        let previous = previous_assignments.get(output).map_or("", String::as_str);
        if !is_safe_positional(path) || !is_safe_positional(previous) {
            log::warn!("reconcile {output}: refusing path beginning with '-' (argv injection)");
            continue;
        }
        match kind {
            wall_proto::kind::VIDEO => {
                if multi_video_handled {
                    continue;
                }
                if let Some(handoff) = reconcile_video(
                    state,
                    VideoReconcileRequest {
                        output,
                        path,
                        previous,
                        transition: transitions_for_output(
                            transition.is_some(),
                            transition_primary.as_deref(),
                            output,
                        )
                        .then_some(transition)
                        .flatten(),
                        mute,
                        volume,
                        reuse,
                    },
                )? {
                    pending.push(handoff);
                }
                keep_video.push(output.clone());
            }
            wall_proto::kind::STATIC => {
                if static_handled.contains(output) {
                    continue;
                }
                if let Some(handoff) = reconcile_static(
                    state,
                    StaticReconcileRequest { output, path, previous, reuse },
                )? {
                    pending.push(handoff);
                }
                keep_still.push(output.clone());
            }
            wall_proto::kind::WE => group_output(output, we_id, &mut we_groups),
            other => log::warn!("reconcile {output}: unknown type '{other}', skipping"),
        }
    }

    for outputs in we_groups.values() {
        keep_video.push(crate::we::scene_renderer_key(outputs));
    }
    keep_video.sort();
    keep_video.dedup();
    let we_audio = super::resolver::resolve_we_audio(&map, &we_groups);
    let prepared_we = prepare_we(state, we_groups, we_audio)?;
    let mut we_assignments = Vec::new();
    for output in &targets {
        let Some(entry) = map.get(output) else { continue };
        if entry.get("type").and_then(serde_json::Value::as_str) == Some(wall_proto::kind::WE) {
            let path = entry.get("path").and_then(serde_json::Value::as_str).unwrap_or("");
            let we_id = entry.get("we_id").and_then(serde_json::Value::as_str).unwrap_or("");
            we_assignments
                .push((output.clone(), if path.is_empty() { we_id } else { path }.to_string()));
        }
    }

    let mut prepared_batch = prepare_batch(pending, prepared_we, overlays)?;
    prepared_batch.start_overlays()?;
    prepared_batch.finalize(state);
    for (output, path) in we_assignments {
        state.renderers().set_assignment(&output, &path);
    }
    state.renderers().retain_video_papers(&keep_video);
    state.renderers().retain_output_stills(&keep_still);
    state.renderers().kill_base_still();
    state.renderers().kill_video_paper("*");
    state.renderers().kill_paper();

    if intent.updates_audio_state() {
        crate::audio::mute_dedup_losers(state, &cache);
    }
    super::policy::record_paper_policy(state);
    Ok(())
}

fn warn_unlisted_targets(monitors: &[String], targets: &[String]) {
    for target in targets {
        if !monitors.iter().any(|monitor| monitor == target) {
            log::warn!(
                "reconcile: outputs.json has '{target}' missing from live enumerate {monitors:?}; rendering anyway"
            );
        }
    }
}

#[cfg(test)]
mod tests;
