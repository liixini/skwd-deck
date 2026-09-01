//! Public apply map.
//!
//! This module deliberately owns no renderer process or media policy. The
//! failure boundaries are:
//!
//! - `engine`: owns backend selection/dispatch, not native renderer lifecycle.
//! - `resolver`: recovers canonical sources without launching a renderer.
//! - `static_media`, `video_media`, `wallpaper_engine`: own media behavior.
//! - `transition`: turns configured/explicit intent into an immutable plan.
//! - `launch`: owns a candidate plus every displaced incumbent until commit;
//!   dropping it is cancellation and restores the incumbents.
//! - `transaction`: commits ready renderer handoffs and their assignments.
//! - `reconcile`: prepares every output/WE/overlay candidate before its batch
//!   commits, so concurrent output replacement has one all-or-nothing boundary.
//! - media owners mutate their steady renderer state; `lifecycle` provides the
//!   shared persistence, validation, and deferred-retirement operations.

use crate::state::WallState;

use super::lifecycle::validate_source;
use super::transition::TransitionSelection;

pub use super::engine::{VideoEngine, video_engine_is_vk};
pub use super::policy::{
    active_renderer_policy_matches, native_scene_properties_match, paper_policy_matches,
    renderer_policy_matches, scene_properties_signature,
};
pub use super::refresh::refresh_renderer_policy;
pub use super::resolver::{resolve_current_image, resolve_current_video, resolve_we_from_state};
pub use super::static_media::{apply_static_smart, apply_static_transition};
pub use super::video_media::{apply_video, apply_video_transition};
pub use super::wallpaper_engine::reload_we;

#[allow(clippy::too_many_arguments)]
pub fn apply_static(
    state: &std::sync::Arc<WallState>,
    output: &str,
    path: &str,
    fill_mode: &str,
    from: Option<&str>,
    transition_enabled: bool,
    shader: &str,
    duration_ms: u64,
) -> anyhow::Result<()> {
    let resolved = super::resolver::resolve_current_image(path);
    if output != "*" && !crate::plasma::available() {
        if let Some(result) =
            super::engine::apply_static_override(state, output, &resolved, fill_mode)
        {
            return result;
        }
        return apply_output_with_transition(
            state,
            output,
            wall_proto::kind::STATIC,
            &resolved,
            "",
            fill_mode,
            false,
            0,
            Some(crate::backend::wallpaper::OutputTransitionRequest {
                enabled: transition_enabled,
                shader,
                duration_ms,
            }),
        );
    }
    let transition =
        TransitionSelection::Explicit { enabled: transition_enabled, shader, duration_ms }
            .resolve(state);
    super::static_media::apply_static_owned(
        state,
        super::static_media::StaticApplyRequest {
            output,
            path: &resolved,
            fill_mode,
            from,
            transition,
        },
    )
}

#[allow(clippy::too_many_arguments)]
pub fn apply_output(
    state: &WallState,
    output: &str,
    ty: &str,
    path: &str,
    we_id: &str,
    fill_mode: &str,
    mute: bool,
    volume: u32,
) -> anyhow::Result<()> {
    apply_output_with_transition(state, output, ty, path, we_id, fill_mode, mute, volume, None)
}

#[allow(clippy::too_many_arguments)]
pub fn apply_output_with_transition(
    state: &WallState,
    output: &str,
    ty: &str,
    path: &str,
    we_id: &str,
    _fill_mode: &str,
    mute: bool,
    volume: u32,
    transition_request: Option<crate::backend::wallpaper::OutputTransitionRequest<'_>>,
) -> anyhow::Result<()> {
    validate_source(path)?;
    crate::awww::stop();
    let cache = state.config().cache_dir();
    let monitors = crate::outputs::names();
    let monitors: Vec<String> =
        if monitors.is_empty() { vec![output.to_string()] } else { monitors };
    log::info!(
        "apply_output: target={output} type={ty} path={path} we={we_id} monitors={monitors:?}"
    );
    let previous = crate::audio::read_state(&cache);
    crate::audio::expand_wildcard(&cache, &monitors);
    crate::audio::set_entry(&cache, output, ty, path, we_id, mute, volume);
    let transition = match transition_request {
        Some(request) => TransitionSelection::Explicit {
            enabled: request.enabled,
            shader: request.shader,
            duration_ms: request.duration_ms,
        },
        None => TransitionSelection::Configured,
    }
    .resolve(state);
    let result = super::reconcile::reconcile_outputs(
        state,
        &monitors,
        &super::reconcile::ReconcileIntent::Apply { transition },
    );
    if result.is_err() {
        crate::audio::write_state(&cache, &previous);
    }
    result
}
