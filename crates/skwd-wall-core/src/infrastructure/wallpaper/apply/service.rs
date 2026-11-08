use super::reconcile::{ReconcileIntent, reconcile_outputs};
use crate::state::WallState;

pub(super) fn policy_matches(state: &WallState, current: &serde_json::Value) -> bool {
    use crate::infrastructure::paper::{PaperClient, Startup};

    if state.renderers().policy(super::policy::PAPER_POLICY_KEY).is_none()
        || !super::policy::paper_policy_matches(state)
        || !crate::infrastructure::paper::paper_socket_path().exists()
    {
        return false;
    }
    let config = state.config().clone();
    let Ok(status) = PaperClient::configured(&config).status(Startup::ExistingOnly) else {
        return false;
    };
    let scenes = serde_json::Value::Object(
        current
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(_, entry)| {
                entry.get(wall_proto::apply_fields::TYPE).and_then(serde_json::Value::as_str)
                    == Some(wall_proto::kind::WE)
            })
            .map(|(output, entry)| (output.clone(), entry.clone()))
            .collect(),
    );
    let Ok(expected) = plan(state, &scenes, &crate::outputs::enumerate(), &status) else {
        return false;
    };
    scenes_match(&expected, &status)
}

pub(super) fn scenes_match(
    expected: &paper_control::ApplyRequest,
    status: &paper_control::StatusResult,
) -> bool {
    let (Some(expected_policy), Some(actual_policy)) = (&expected.policy, &status.policy) else {
        return false;
    };
    let mut expected_policy = expected_policy.clone();
    let mut actual_policy = actual_policy.clone();
    expected_policy.transition_fps = expected_policy.transition_fps.filter(|fps| *fps > 0);
    actual_policy.transition_fps = actual_policy.transition_fps.filter(|fps| *fps > 0);
    expected_policy == actual_policy
        && expected.assignments.iter().all(|desired| {
            status.assignments.iter().any(|actual| {
                actual.ready
                    && actual.pid.is_some()
                    && actual.outputs == desired.outputs
                    && actual.source == desired.source
                    && actual.fill_mode == desired.fill_mode
                    && actual.background == desired.background
                    && actual.layer == desired.layer
            })
        })
}

pub(super) fn refresh(state: &WallState, current: &serde_json::Value) -> anyhow::Result<bool> {
    use crate::infrastructure::paper::{PaperClient, PaperClientAdapter, Startup};

    if !crate::infrastructure::paper::paper_socket_path().exists() {
        return Ok(false);
    }
    let signature = super::policy::paper_policy_signature(state);
    let paper_changed = !super::policy::paper_policy_matches(state);
    let scene_changed = !super::policy::native_scene_policy_matches(state);
    let config = state.config().clone();
    let status = PaperClient::configured(&config).status(Startup::ExistingOnly)?;
    if status.assignments.is_empty() {
        return Ok(false);
    }
    let outputs = crate::outputs::enumerate();
    let mut request = plan(state, current, &outputs, &status)?;
    request.assignments.retain(|assignment| {
        status.assignments.iter().any(|owned| {
            owned.outputs.iter().any(|output| {
                output == wall_proto::apply_fields::ALL_OUTPUTS
                    || assignment.outputs.contains(output)
            })
        })
    });

    anyhow::ensure!(!request.assignments.is_empty(), "Paper policy refresh has no owned outputs");
    let owned = request
        .assignments
        .iter()
        .flat_map(|assignment| assignment.outputs.clone())
        .collect::<Vec<_>>();
    PaperClientAdapter::configured(&config).apply_request(request)?;
    state.renderers().retire_shared_outputs(&owned)?;
    for key in state.renderers().video_paper_outputs() {
        let target = paper_control::OutputTarget::from_arg(&key);
        let covered =
            outputs.iter().filter(|output| target.matches(&output.name)).collect::<Vec<_>>();
        if !covered.is_empty() && covered.iter().all(|output| owned.contains(&output.name)) {
            state.renderers().kill_video_paper(&key);
        }
    }
    for output in &owned {
        state.renderers().kill_output_still(output);
    }
    let local = outputs
        .iter()
        .filter(|output| !owned.contains(&output.name))
        .filter_map(|output| {
            current
                .get(&output.name)
                .or_else(|| current.get(wall_proto::apply_fields::ALL_OUTPUTS))
                .map(|entry| (output.name.clone(), entry.clone()))
        })
        .collect::<serde_json::Map<_, _>>();
    if paper_changed && !local.is_empty() {
        let monitors = outputs.iter().map(|output| output.name.clone()).collect::<Vec<_>>();
        reconcile_outputs(
            state,
            &monitors,
            &ReconcileIntent::PolicyRefresh,
            super::reconcile::ReconcileScope::Exclude(&owned),
        )?;
    } else if scene_changed {
        let (groups, audio) = super::resolver::resolve_we_from_state(&local);
        if !groups.is_empty() {
            super::wallpaper_engine::rebuild_we(state, groups, audio)?;
        }
    }
    state.renderers().set_policy(super::policy::PAPER_POLICY_KEY, &signature);
    Ok(true)
}

pub(super) fn plan(
    state: &WallState,
    current: &serde_json::Value,
    outputs: &[crate::outputs::OutputInfo],
    status: &paper_control::StatusResult,
) -> anyhow::Result<paper_control::ApplyRequest> {
    use crate::infrastructure::paper::{PaperClientAdapter, PaperCompositionPlan};
    use wall_proto::apply_fields::ALL_OUTPUTS;

    let expanded = serde_json::Value::Object(
        outputs
            .iter()
            .filter_map(|output| {
                current
                    .get(&output.name)
                    .or_else(|| current.get(ALL_OUTPUTS))
                    .map(|entry| (output.name.clone(), entry.clone()))
            })
            .collect(),
    );
    let config = state.config().clone();
    let PaperCompositionPlan::Replace(mut request) =
        PaperClientAdapter::configured(&config).composition_plan(&config, &expanded, outputs)?
    else {
        anyhow::bail!("Paper policy refresh has no desired assignments");
    };
    crate::infrastructure::paper::sharing(state, &mut request);
    for assignment in &mut request.assignments {
        if let Some(previous) = status.assignments.iter().find(|previous| {
            previous.source.path == assignment.source.path
                && previous.source.kind == assignment.source.kind
                && previous
                    .outputs
                    .iter()
                    .any(|output| output == ALL_OUTPUTS || assignment.outputs.contains(output))
        }) {
            assignment.source.properties = previous.source.properties.clone();
        }
    }
    Ok(request)
}
