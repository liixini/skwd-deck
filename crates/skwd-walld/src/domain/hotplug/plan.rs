#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AppliedKind {
    Static,
    Video,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HotplugPlan {
    KeepAlive,
    RespawnUniformVideo,
    RespawnUniformStatic,
    Reconcile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Representative {
    pub(crate) output: String,
    pub(crate) state_key: String,
}

pub(crate) fn per_output_state_is_divergent(persisted_outputs: &[String]) -> bool {
    !persisted_outputs.is_empty() && !persisted_outputs.iter().any(|output| output == "*")
}

pub(crate) fn hotplug_plan(
    persisted_outputs: &[String],
    uniform_kind: Option<AppliedKind>,
    engine_is_vk: bool,
    video_alive: bool,
    still_alive: bool,
) -> HotplugPlan {
    if persisted_outputs.len() != 1 || persisted_outputs[0] != "*" {
        return HotplugPlan::Reconcile;
    }

    match uniform_kind {
        Some(AppliedKind::Video) if engine_is_vk || !video_alive => {
            HotplugPlan::RespawnUniformVideo
        }
        Some(AppliedKind::Video) => HotplugPlan::KeepAlive,
        Some(AppliedKind::Static) if still_alive => HotplugPlan::KeepAlive,
        Some(AppliedKind::Static) => HotplugPlan::RespawnUniformStatic,
        Some(AppliedKind::Other) | None => HotplugPlan::Reconcile,
    }
}

pub(crate) fn newly_appeared_outputs(
    persisted_outputs: &[String],
    live_outputs: &[String],
) -> Vec<String> {
    live_outputs.iter().filter(|output| !persisted_outputs.contains(output)).cloned().collect()
}

pub(crate) fn retained_outputs(
    persisted_outputs: &[String],
    live_outputs: &[String],
) -> Vec<String> {
    persisted_outputs
        .iter()
        .filter(|output| output.as_str() == "*" || live_outputs.contains(output))
        .cloned()
        .collect()
}

pub(crate) fn representative(
    persisted_outputs: &[String],
    live_outputs: &[String],
) -> Option<Representative> {
    if let Some(output) = live_outputs.iter().find(|output| persisted_outputs.contains(output)) {
        return Some(Representative { output: output.clone(), state_key: output.clone() });
    }

    if persisted_outputs.iter().any(|output| output == "*") {
        return live_outputs
            .first()
            .map(|output| Representative { output: output.clone(), state_key: String::from("*") });
    }

    persisted_outputs
        .first()
        .map(|output| Representative { output: output.clone(), state_key: output.clone() })
}
