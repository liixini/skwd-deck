mod plan;

pub(crate) use plan::{
    AppliedKind, HotplugPlan, hotplug_plan, newly_appeared_outputs, per_output_state_is_divergent,
    representative, retained_outputs,
};

#[cfg(test)]
use plan::Representative;

#[cfg(test)]
mod tests;
