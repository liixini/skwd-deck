use crate::state::WallState;

use super::reconcile::{ReconcileIntent, reconcile_outputs};

pub fn release_outputs(state: &WallState, outputs: &[String]) -> anyhow::Result<()> {
    anyhow::ensure!(!outputs.is_empty(), "external handoff requires at least one output");
    let cache = state.config().cache_dir();
    let previous = crate::audio::read_state(&cache);
    let mut remaining = previous.as_object().cloned().unwrap_or_default();
    let mut assignments = state.renderers().assignments();
    if outputs.iter().any(|output| output == "*") {
        state.renderers().kill_all();
        remaining.clear();
        assignments.clear();
    } else {
        let monitors = crate::outputs::names();
        if let Some(wildcard) = remaining.remove("*") {
            anyhow::ensure!(
                !monitors.is_empty(),
                "cannot release one output without the live display list"
            );
            for monitor in &monitors {
                remaining.entry(monitor.clone()).or_insert_with(|| wildcard.clone());
            }
        }
        remaining.retain(|output, _| !outputs.contains(output));
        crate::audio::write_state(&cache, &serde_json::Value::Object(remaining.clone()));
        let native_active = !state.renderers().wallpaper_pids().is_empty()
            || state.renderers().renderer_alive(wall_proto::kind::WE);
        if native_active && !remaining.is_empty() {
            if let Err(error) = reconcile_outputs(state, &monitors, &ReconcileIntent::PolicyRefresh)
            {
                crate::audio::write_state(&cache, &previous);
                return Err(error);
            }
        } else if remaining.is_empty()
            && previous.as_object().is_some_and(|map| {
                outputs.iter().any(|output| map.contains_key(output)) || map.contains_key("*")
            })
        {
            state.renderers().kill_all();
        }
        for output in outputs {
            state.renderers().kill_video_paper(output);
            state.renderers().kill_output_still(output);
        }
        assignments = state.renderers().assignments();
        assignments.retain(|output, _| !outputs.contains(output));
        assignments.remove("*");
    }
    crate::audio::write_state(&cache, &serde_json::Value::Object(remaining));
    state.renderers().replace_assignments(assignments);
    Ok(())
}
