use crate::state::WallState;

use super::launch::{PreparedRenderer, ReadyRenderer};
use super::lifecycle::allow_transition_to_finish;

#[derive(Clone, Copy)]
pub(super) enum ReusePolicy {
    WarmAllowed,
    ColdOnly,
}

impl ReusePolicy {
    pub(super) fn allows_warm(self) -> bool {
        matches!(self, Self::WarmAllowed)
    }
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
            renderer: self.renderer.prepare_commit()?,
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
