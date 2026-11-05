use anyhow::{Context, Result, ensure};
use paper_control::{ApplyRequest, Source, SourceKind, StatusResult, TransitionPolicy};
use serde_json::Value;
use wall_proto::apply_fields::{ALL_OUTPUTS, PATH};

use crate::backend::wallpaper::{ApplyOutputRequest, OutputTransitionRequest, StaticSmartRequest};
use crate::infrastructure::paper::{
    PaperClient, PaperClientAdapter, PaperCompositionPlan, Startup,
};
use crate::state::WallState;

pub(super) fn image(
    state: &WallState,
    request: StaticSmartRequest<'_>,
    transition: OutputTransitionRequest<'_>,
    targets: &[String],
) -> Result<bool> {
    let config = state.config();
    let (mute, volume) = crate::audio::resolve_defaults(
        &config.cache_dir(),
        request.output,
        config.renderer().mute(),
        config.renderer().volume(),
    );
    apply(
        state,
        ApplyOutputRequest {
            output: request.output,
            kind: wall_proto::kind::STATIC,
            path: request.path,
            we_id: "",
            fill_mode: request.fill_mode,
            mute,
            volume,
            transition: Some(transition),
            fanout: None,
        },
        targets,
    )
}

pub(super) fn apply(
    state: &WallState,
    request: ApplyOutputRequest<'_>,
    targets: &[String],
) -> Result<bool> {
    if matches!(request.kind, wall_proto::kind::VIDEO | wall_proto::kind::WE)
        && super::drawing::apply_media(state, request)?
    {
        return Ok(true);
    }
    crate::plasma::require_backend()?;
    if crate::plasma::available() {
        return Ok(false);
    }
    let socket = crate::infrastructure::paper::paper_socket_path();
    if !socket.exists() {
        return Ok(false);
    }
    let config = state.config();
    let status = PaperClient::configured(&config).status(Startup::Allowed)?;
    if status.assignments.is_empty() {
        return Ok(false);
    }
    let outputs = crate::outputs::enumerate();
    if !can_handoff(&config, &outputs, &status, request, targets) {
        return Ok(false);
    }
    let previous = crate::audio::read_state(&config.cache_dir());
    let (apply, candidate, selected) = plan(state, &outputs, &previous, &status, request, targets)?;
    PaperClientAdapter::configured(&config).apply_request(apply)?;
    crate::audio::write_state(&config.cache_dir(), &candidate);
    for output in selected {
        let path = if request.kind == wall_proto::kind::WE {
            config.we_dir().join(request.we_id).display().to_string()
        } else {
            candidate[&output][PATH].as_str().context("Paper assignment has no path")?.to_string()
        };
        state.renderers().set_assignment(&output, &path);
    }
    Ok(true)
}

fn can_handoff(
    config: &crate::config::Config,
    outputs: &[crate::outputs::OutputInfo],
    status: &StatusResult,
    request: ApplyOutputRequest<'_>,
    targets: &[String],
) -> bool {
    let transition =
        request.transition.map_or_else(|| config.transition().active(), |request| request.enabled);
    if transition
        && status.policy.as_ref().and_then(|policy| policy.transitions_enabled) == Some(false)
    {
        return false;
    }
    let selected = selected_outputs(config, outputs, request, targets);
    !selected.is_empty()
        && selected.iter().all(|output| {
            status.assignments.iter().any(|assignment| {
                assignment.outputs.iter().any(|owned| owned == ALL_OUTPUTS || owned == &output.name)
            })
        })
}

fn plan(
    state: &WallState,
    outputs: &[crate::outputs::OutputInfo],
    previous: &Value,
    status: &StatusResult,
    request: ApplyOutputRequest<'_>,
    targets: &[String],
) -> Result<(ApplyRequest, Value, Vec<String>)> {
    let config = state.config();
    let selected = selected_outputs(&config, outputs, request, targets)
        .into_iter()
        .map(|output| output.name.clone())
        .collect::<Vec<_>>();
    ensure!(!selected.is_empty(), "Paper requires an unlocked live output");
    let shader = config.transition().shader();
    let transition =
        request.transition.unwrap_or(crate::backend::wallpaper::OutputTransitionRequest {
            enabled: config.transition().active(),
            shader: &shader,
            duration_ms: config.transition().duration_ms(),
        });
    let transitioning = transition.enabled && !state.apply().no_transition();
    let mut candidate = previous.clone();
    let entries = candidate.as_object_mut().context("audio state must be an object")?;
    if let Some(wildcard) = entries.remove(ALL_OUTPUTS) {
        for output in outputs {
            entries.entry(output.name.clone()).or_insert_with(|| wildcard.clone());
        }
    }
    for output in &selected {
        entries.insert(
            output.clone(),
            crate::audio::entry(
                request.kind,
                request.path,
                request.we_id,
                request.mute,
                request.volume,
            ),
        );
    }
    let adapter = PaperClientAdapter::configured(&config);
    let PaperCompositionPlan::Replace(mut apply) =
        adapter.composition_plan(&config, &candidate, outputs)?
    else {
        anyhow::bail!("Paper composition has no assignments");
    };
    apply.replace_all = status
        .assignments
        .iter()
        .any(|assignment| assignment.outputs.iter().any(|output| output == ALL_OUTPUTS));
    apply.policy = None;
    let dedup_mute = if transitioning && request.kind == wall_proto::kind::VIDEO {
        let effective = effective_audio_state(&candidate, &apply, status, &selected)?;
        crate::audio::compute_dedup_from(
            &effective,
            &selected,
            &std::collections::HashMap::<String, bool>::new(),
            request.kind,
            request.path,
            request.we_id,
            request.mute,
        )
    } else {
        std::collections::HashMap::new()
    };
    if !apply.replace_all {
        apply
            .assignments
            .retain(|assignment| assignment.outputs.iter().any(|output| selected.contains(output)));
    }
    for assignment in &mut apply.assignments {
        if transitioning && assignment.outputs.iter().any(|output| selected.contains(output)) {
            if request.kind == wall_proto::kind::VIDEO {
                assignment.mute = assignment
                    .outputs
                    .first()
                    .and_then(|output| dedup_mute.get(output))
                    .copied()
                    .unwrap_or(request.mute);
            }
            assignment.transition = Some(TransitionPolicy {
                drawing: None,
                drawing_source: None,
                from: None,
                effect: Some(transition.shader.to_string()),
                duration_ms: Some(transition.duration_ms),
                fps: (config.transition().fps() > 0).then(|| config.transition().fps()),
            });
        }
    }
    apply.validate().map_err(anyhow::Error::msg)?;
    Ok((apply, candidate, selected))
}

fn effective_audio_state(
    candidate: &Value,
    apply: &ApplyRequest,
    status: &StatusResult,
    selected: &[String],
) -> Result<Value> {
    let mut effective = candidate.clone();
    let entries = effective.as_object_mut().context("audio state must be an object")?;
    entries.retain(|output, _| selected.contains(output));
    let mut record_video = |outputs: &[String], source: &Source, mute: bool, volume: u32| {
        if source.kind != SourceKind::Video {
            return;
        }
        for output in outputs {
            if !selected.contains(output) {
                entries.insert(
                    output.clone(),
                    crate::audio::entry(wall_proto::kind::VIDEO, &source.path, "", mute, volume),
                );
            }
        }
    };
    if apply.replace_all {
        for assignment in &apply.assignments {
            record_video(
                &assignment.outputs,
                &assignment.source,
                assignment.mute,
                assignment.volume,
            );
        }
    } else {
        for assignment in &status.assignments {
            record_video(
                &assignment.outputs,
                &assignment.source,
                assignment.mute,
                assignment.volume,
            );
        }
    }
    Ok(effective)
}

fn selected_outputs<'a>(
    config: &crate::config::Config,
    outputs: &'a [crate::outputs::OutputInfo],
    request: ApplyOutputRequest<'_>,
    targets: &[String],
) -> Vec<&'a crate::outputs::OutputInfo> {
    outputs
        .iter()
        .filter(|output| {
            (if targets.is_empty() {
                request.output == ALL_OUTPUTS || request.output == output.name
            } else {
                targets.iter().any(|target| target == ALL_OUTPUTS || target == &output.name)
            }) && !config.display().output_locked(&output.name)
        })
        .collect()
}

#[cfg(test)]
mod tests;
