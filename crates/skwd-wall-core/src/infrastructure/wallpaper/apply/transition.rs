use crate::domain::wallpaper::transition_args_for;
use crate::state::WallState;

/// A named transition request replaces the old optional tuple/flag matrix.
/// Media owners choose whether configuration or an explicit caller request is
/// authoritative; this owner resolves it to one immutable plan.
pub(super) enum TransitionSelection<'a> {
    Configured,
    Explicit { enabled: bool, shader: &'a str, duration_ms: u64 },
}

#[derive(Clone)]
pub(super) struct TransitionPlan {
    enabled: bool,
    shader: String,
    duration_ms: u64,
}

impl TransitionPlan {
    pub(super) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(super) fn shader(&self) -> &str {
        &self.shader
    }

    pub(super) fn duration_ms(&self) -> u64 {
        self.duration_ms
    }
}

pub(super) struct OverlayPlan {
    pub(super) output: String,
    pub(super) args: Vec<String>,
}

impl TransitionSelection<'_> {
    pub(super) fn resolve(self, state: &WallState) -> TransitionPlan {
        let no_transition = state.apply().no_transition();
        match self {
            Self::Configured => {
                let config = state.config();
                TransitionPlan {
                    enabled: config.transition().active() && !no_transition,
                    shader: config.transition().shader(),
                    duration_ms: config.transition().duration_ms(),
                }
            }
            Self::Explicit { enabled, shader, duration_ms } => TransitionPlan {
                enabled: enabled && !no_transition,
                shader: shader.to_string(),
                duration_ms,
            },
        }
    }
}

pub(super) fn transition_primary(
    state: &WallState,
    outputs: &[String],
    shader: &str,
) -> Option<String> {
    let config = state.config();
    if config.transition().scope(shader) != "primary" {
        return None;
    }
    let configured = config.transition().sand_primary();
    outputs
        .iter()
        .find(|output| !configured.is_empty() && output.as_str() == configured)
        .or_else(|| outputs.first())
        .cloned()
}

pub(super) fn transitions_for_output(enabled: bool, primary: Option<&str>, output: &str) -> bool {
    enabled && primary.is_none_or(|primary| primary == output)
}

pub(super) fn static_overlay_plan(
    output: &str,
    from: &str,
    to: &str,
    fill_mode: &str,
    shader: &str,
    duration_ms: u64,
) -> OverlayPlan {
    OverlayPlan {
        output: output.to_string(),
        args: transition_args_for(output, from, to, fill_mode, shader, duration_ms),
    }
}

pub(super) fn static_overlay_plans(
    state: &WallState,
    map: &serde_json::Map<String, serde_json::Value>,
    targets: &[String],
    previous_assignments: &std::collections::HashMap<String, String>,
    transition: Option<&TransitionPlan>,
    transition_primary: Option<&str>,
) -> Vec<OverlayPlan> {
    let Some(plan) = transition else {
        return Vec::new();
    };
    targets
        .iter()
        .filter_map(|output| {
            if !transitions_for_output(true, transition_primary, output) {
                log::debug!("overlay {output}: not the transition primary, skipping");
                return None;
            }
            let entry = map.get(output)?;
            if entry.get("type").and_then(serde_json::Value::as_str)
                != Some(wall_proto::kind::STATIC)
            {
                return None;
            }
            let path = entry.get("path").and_then(serde_json::Value::as_str).unwrap_or("");
            let previous = previous_assignments.get(output).map_or("", String::as_str);
            if path.is_empty()
                || previous == path
                || !crate::domain::wallpaper::is_safe_positional(path)
                || !crate::domain::wallpaper::is_safe_positional(previous)
            {
                log::debug!("overlay {output}: no plan (previous={previous:?} path={path:?})");
                return None;
            }
            let Some(from) = previous_media_source(state, output, previous) else {
                log::debug!("overlay {output}: previous {previous:?} yields no media source");
                return None;
            };
            if from == path {
                log::debug!("overlay {output}: source equals destination, skipping");
                return None;
            }
            Some(static_overlay_plan(
                output,
                &from,
                path,
                &state.config().display().fill_mode_for(output),
                plan.shader(),
                plan.duration_ms(),
            ))
        })
        .collect()
}

pub(super) fn previous_media_source(
    state: &WallState,
    output: &str,
    previous: &str,
) -> Option<String> {
    if previous.is_empty() {
        return None;
    }
    if let Some(frame) = crate::we::capture_transition_frame(state, output) {
        return Some(frame);
    }
    let path = std::path::Path::new(previous);
    if path.is_file() {
        return Some(previous.to_string());
    }
    if path.is_dir() {
        return crate::we::find_preview(path).map(|preview| preview.display().to_string());
    }
    crate::we::valid_we_id(previous)
        .then(|| state.config().we_dir().join(previous))
        .and_then(|directory| crate::we::find_preview(&directory))
        .map(|preview| preview.display().to_string())
}
