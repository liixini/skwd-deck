#[derive(Clone, Copy)]
pub(in crate::infrastructure::wallpaper::apply) enum VideoPlayback<'a> {
    Steady,
    Transition {
        from: &'a str,
        plan: &'a crate::infrastructure::wallpaper::apply::transition::TransitionPlan,
    },
}

impl VideoPlayback<'_> {
    pub(super) fn duration_ms(&self) -> Option<u64> {
        match self {
            Self::Steady => None,
            Self::Transition { plan, .. } => Some(plan.duration_ms()),
        }
    }
}
