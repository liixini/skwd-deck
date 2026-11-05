use crate::state::WallState;

use super::launch::{PreparedRenderer, ReadyRenderer};
use super::lifecycle::allow_transition_to_finish;

#[derive(Clone, Copy)]
pub(super) enum ReusePolicy {
    WarmAllowed,
    ColdOnly,
    PrepareHidden,
}

impl ReusePolicy {
    pub(super) fn prepares_hidden(self) -> bool {
        matches!(self, Self::PrepareHidden)
    }

    pub(super) fn allows_warm(self) -> bool {
        matches!(self, Self::WarmAllowed)
    }

    pub(super) fn for_changes(
        state: &WallState,
        map: &serde_json::Map<String, serde_json::Value>,
        targets: &[String],
        previous_assignments: &std::collections::HashMap<String, String>,
        policy: &super::policy::PolicyGate<'_>,
    ) -> Self {
        let changing = targets
            .iter()
            .filter(|output| {
                map.get(*output).is_some_and(|entry| {
                    changes_renderer(state, map, previous_assignments, output, entry)
                })
            })
            .count();
        if changing <= 1 && policy.current() { Self::WarmAllowed } else { Self::ColdOnly }
    }
}

fn changes_renderer(
    state: &WallState,
    map: &serde_json::Map<String, serde_json::Value>,
    previous_assignments: &std::collections::HashMap<String, String>,
    output: &str,
    entry: &serde_json::Value,
) -> bool {
    match entry.get(wall_proto::apply_fields::TYPE).and_then(serde_json::Value::as_str) {
        Some(wall_proto::kind::STATIC) => {
            !still_unchanged(state, map, previous_assignments, output)
        }
        Some(wall_proto::kind::VIDEO) => {
            !path_unchanged(map, previous_assignments, output)
                || !state.renderers().has_video_paper(output)
        }
        Some(wall_proto::kind::WE) => true,
        _ => false,
    }
}

fn path_unchanged(
    map: &serde_json::Map<String, serde_json::Value>,
    previous_assignments: &std::collections::HashMap<String, String>,
    output: &str,
) -> bool {
    let path = map
        .get(output)
        .and_then(|entry| entry.get(wall_proto::apply_fields::PATH))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    previous_assignments.get(output).is_some_and(|previous| previous == path)
}

fn still_unchanged(
    state: &WallState,
    map: &serde_json::Map<String, serde_json::Value>,
    previous_assignments: &std::collections::HashMap<String, String>,
    output: &str,
) -> bool {
    let Some(key) = state.renderers().output_still_key(output) else {
        return false;
    };
    super::still_policy::still_kept(state, &key)
        && key.split(',').all(|member| path_unchanged(map, previous_assignments, member))
}

/// A renderer candidate and the state mutations which become valid only once
/// every candidate in the reconciliation batch is ready.
pub(super) struct ReadyHandoff<'a> {
    pub(super) renderer: ReadyRenderer<'a>,
    pub(super) assignments: Vec<(String, String)>,
    pub(super) transition_duration: Option<u64>,
}

impl<'a> ReadyHandoff<'a> {
    pub(super) fn prepare_commit(self) -> anyhow::Result<PreparedHandoff<'a>> {
        Ok(PreparedHandoff {
            renderer: self.renderer.reveal_still()?.prepare_commit()?,
            assignments: self.assignments,
            transition_duration: self.transition_duration,
        })
    }
}

pub(super) struct PreparedHandoff<'a> {
    renderer: PreparedRenderer<'a>,
    assignments: Vec<(String, String)>,
    transition_duration: Option<u64>,
}

impl PreparedHandoff<'_> {
    pub(super) fn finalize(self, state: &WallState) {
        let pid = self.renderer.finalize();
        if let Some(duration) = self.transition_duration {
            allow_transition_to_finish(state, pid, duration);
        }
        for (output, path) in self.assignments {
            state.renderers().set_assignment(&output, &path);
        }
    }
}
