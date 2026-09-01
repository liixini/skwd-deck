use crate::state::WallState;

use super::reconcile::{ReconcileIntent, reconcile_outputs};
use super::video_media::{StateRecording, VideoApplyRequest, apply_video_request};

/// Rebuilds active renderers after a policy change without rewriting desired
/// wallpaper/audio state.
pub fn refresh_renderer_policy(state: &WallState) -> anyhow::Result<()> {
    let _apply = state.apply().lock();
    let current = crate::audio::read_state(&state.config().cache_dir());
    let Some(outputs) = current.as_object().filter(|outputs| !outputs.is_empty()) else {
        return Ok(());
    };
    if let Some(entry) = outputs.get("*") {
        let kind = entry.get("type").and_then(serde_json::Value::as_str).unwrap_or("");
        let path = entry.get("path").and_then(serde_json::Value::as_str).unwrap_or("");
        let we_id = entry.get("we_id").and_then(serde_json::Value::as_str).unwrap_or("");
        let mute = entry.get("mute").and_then(serde_json::Value::as_bool).unwrap_or(true);
        let volume =
            entry.get("volume").and_then(serde_json::Value::as_u64).unwrap_or(100).min(100) as u32;
        return match kind {
            wall_proto::kind::VIDEO if !path.is_empty() => {
                let fill_mode = state.config().display().fill_mode();
                apply_video_request(
                    state,
                    VideoApplyRequest {
                        output: "*",
                        path,
                        fill_mode: &fill_mode,
                        mute,
                        volume,
                        recording: StateRecording::PreserveExisting,
                    },
                )
            }
            wall_proto::kind::WE if !we_id.is_empty() => super::wallpaper_engine::reload_we(state),
            _ => Ok(()),
        };
    }
    let mut monitors = crate::outputs::names();
    if monitors.is_empty() {
        monitors = outputs.keys().filter(|output| output.as_str() != "*").cloned().collect();
        monitors.sort();
    }
    reconcile_outputs(state, &monitors, &ReconcileIntent::PolicyRefresh)
}
