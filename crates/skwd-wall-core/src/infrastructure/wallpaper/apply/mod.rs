mod engine;
mod external;
mod launch;
mod lifecycle;
mod orchestrator;
mod policy;
mod reconcile;
mod refresh;
mod resolver;
mod service;
mod static_media;
mod still_policy;
mod transaction;
mod transition;
mod video_media;
mod wallpaper_engine;

pub use crate::domain::wallpaper::*;
pub(crate) use launch::{
    NATIVE_SCENE_READY_TIMEOUT, PreparedRenderer, ReadyRenderer, RendererLaunchSpec,
};
pub(crate) use lifecycle::spawn_native_scene;
pub use orchestrator::*;
pub(crate) use policy::{
    PAPER_POLICY_KEY, native_scene_policy_matches, paper_policy_signature,
    record_native_scene_policies, record_paper_policy, record_scene_properties,
};
pub(crate) use transition::previous_media_source;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
