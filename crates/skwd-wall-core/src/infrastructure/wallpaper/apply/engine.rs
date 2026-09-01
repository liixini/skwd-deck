use crate::state::WallState;

/// The selected video implementation. Video engine choice is owned here so
/// media owners never need to interpret renderer configuration themselves.
pub struct VideoEngine {
    pub bin: String,
}

pub(super) fn video_engine(state: &WallState) -> VideoEngine {
    VideoEngine { bin: state.config().renderer().vk_bin() }
}

pub const fn video_engine_is_vk(_: &WallState) -> bool {
    true
}

pub(super) fn apply_static_override(
    state: &WallState,
    output: &str,
    path: &str,
    fill_mode: &str,
) -> Option<anyhow::Result<()>> {
    if state.config().renderer().engine() == "awww" && crate::awww::supports(fill_mode) {
        return Some(crate::awww::apply(state, output, path, fill_mode));
    }
    crate::awww::stop();
    None
}
