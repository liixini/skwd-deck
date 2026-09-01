use crate::state::WallState;

use super::engine::video_engine;
use super::launch::{NativeScenePolicy, current_native_scene_policy};

pub(super) const PAPER_POLICY_KEY: &str = "paperpolicy";
pub(super) const NATIVE_SCENE_POLICY_KEY: &str = "scenepolicy";
const NATIVE_SCENE_PROPERTIES_KEY: &str = "sceneproperties";

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub(super) struct PaperPolicy {
    pub(super) performance_mode: bool,
    pub(super) idle_pause_seconds: u32,
    pub(super) renderer_bin: String,
    pub(super) renderer_identity: String,
    pub(super) video_engine: String,
    pub(super) multi_process: bool,
    pub(super) fill_mode: String,
    pub(super) fill_modes: String,
    pub(super) sand_quality: String,
    pub(super) sand_scope: String,
    pub(super) sand_primary: String,
    pub(super) sand_sharp: bool,
    pub(super) sand_fps: String,
    pub(super) output_refresh: String,
    pub(super) transitions_active: bool,
}

impl PaperPolicy {
    pub(super) fn signature(&self) -> String {
        serde_json::json!([
            "v4",
            self.performance_mode,
            self.idle_pause_seconds,
            self.renderer_bin,
            self.renderer_identity,
            self.video_engine,
            self.multi_process,
            self.fill_mode,
            self.fill_modes,
            self.sand_quality,
            self.sand_scope,
            self.sand_primary,
            self.sand_sharp,
            self.sand_fps,
            self.output_refresh,
            self.transitions_active,
        ])
        .to_string()
    }
}

fn renderer_binary_identity(path: &str) -> String {
    use std::os::unix::fs::MetadataExt;

    std::fs::metadata(path).map_or_else(
        |_| "missing".to_string(),
        |metadata| {
            format!(
                "{}:{}:{}:{}:{}",
                metadata.dev(),
                metadata.ino(),
                metadata.size(),
                metadata.mtime(),
                metadata.mtime_nsec(),
            )
        },
    )
}

pub(super) fn current_paper_policy(state: &WallState) -> PaperPolicy {
    let renderer = video_engine(state);
    let config = state.config();
    let renderer_identity = renderer_binary_identity(&renderer.bin);
    let shader = config.transition().shader();
    let sand_scope = if shader.starts_with("sand-") {
        config.transition().scope(&shader)
    } else {
        String::from("all")
    };
    PaperPolicy {
        performance_mode: config.renderer().performance_mode(),
        idle_pause_seconds: config.renderer().idle_pause_seconds(),
        renderer_bin: renderer.bin,
        renderer_identity,
        video_engine: config.renderer().video_engine(),
        multi_process: config.renderer().video_multi_process(),
        fill_mode: config.display().fill_mode(),
        fill_modes: config.display().fill_modes_signature(),
        sand_quality: config.transition().sand_quality(),
        sand_scope,
        sand_primary: config.transition().sand_primary(),
        sand_sharp: config.transition().sand_sharp(),
        sand_fps: config.transition().sand_fps(),
        output_refresh: crate::outputs::refresh_signature(&crate::outputs::enumerate()),
        transitions_active: config.transition().active(),
    }
}

pub fn paper_policy_matches(state: &WallState) -> bool {
    let current = current_paper_policy(state).signature();
    state.renderers().policy(PAPER_POLICY_KEY).is_none_or(|previous| previous == current)
}

pub(super) fn record_paper_policy(state: &WallState) {
    state.renderers().set_policy(PAPER_POLICY_KEY, &current_paper_policy(state).signature());
}

pub(crate) fn native_scene_policy_matches(state: &WallState) -> bool {
    if !paper_policy_matches(state) {
        return false;
    }
    let current = current_native_scene_policy(state).signature();
    state.renderers().policy(NATIVE_SCENE_POLICY_KEY).is_some_and(|previous| previous == current)
}

#[must_use]
pub fn scene_properties_signature(
    entries: &[(String, serde_json::Map<String, serde_json::Value>)],
) -> String {
    let mut rows: Vec<String> = entries
        .iter()
        .map(|(we_id, properties)| {
            let encoded = serde_json::to_string(properties).unwrap_or_default();
            format!("{we_id}={encoded}")
        })
        .collect();
    rows.sort();
    rows.dedup();
    rows.join(";")
}

pub(crate) fn record_scene_properties(state: &WallState, signature: &str) {
    state.renderers().set_policy(NATIVE_SCENE_PROPERTIES_KEY, signature);
}

#[must_use]
pub fn native_scene_properties_match(state: &WallState, we_id: &str) -> bool {
    let overrides = crate::infrastructure::we::scene_overrides(state, we_id);
    let desired = scene_properties_signature(&[(we_id.to_string(), overrides)]);
    state
        .renderers()
        .policy(NATIVE_SCENE_PROPERTIES_KEY)
        .is_some_and(|previous| previous == desired)
}

fn record_native_scene_policy(state: &WallState, policy: &NativeScenePolicy) {
    state.renderers().set_policy(NATIVE_SCENE_POLICY_KEY, &policy.signature());
}

pub(crate) fn record_native_scene_policies(state: &WallState) {
    record_paper_policy(state);
    record_native_scene_policy(state, &current_native_scene_policy(state));
}

pub fn renderer_policy_matches(state: &WallState, kind: &str) -> bool {
    match kind {
        wall_proto::kind::VIDEO => paper_policy_matches(state),
        wall_proto::kind::WE => {
            state.renderers().has_scene_papers() && native_scene_policy_matches(state)
        }
        _ => true,
    }
}

pub fn active_renderer_policy_matches(state: &WallState) -> bool {
    let current = crate::audio::read_state(&state.config().cache_dir());
    let Some(outputs) = current.as_object() else {
        return true;
    };
    let mut needs_paper_policy = false;
    let mut needs_we_policy = false;
    for entry in outputs.values() {
        match entry.get("type").and_then(serde_json::Value::as_str).unwrap_or("") {
            wall_proto::kind::STATIC | wall_proto::kind::VIDEO => needs_paper_policy = true,
            wall_proto::kind::WE => needs_we_policy = true,
            _ => {}
        }
    }
    (!needs_paper_policy || paper_policy_matches(state))
        && (!needs_we_policy || renderer_policy_matches(state, wall_proto::kind::WE))
}
