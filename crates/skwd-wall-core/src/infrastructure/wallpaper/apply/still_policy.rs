use crate::state::WallState;

const STILL_BACKGROUND_PREFIX: &str = "stillbackground:";

pub(super) fn still_surface_key(state: &WallState, output: &str) -> (String, String) {
    let config = state.config();
    let display = config.display();
    let background = if display.background_affects_for(output) {
        serde_json::to_string(&display.background_for(output)).expect("background serializes")
    } else {
        String::new()
    };
    (display.fill_mode_for(output), background)
}

pub(super) fn still_background(state: &WallState, output: &str) -> String {
    serde_json::to_string(&state.config().display().background_for(output))
        .expect("background serializes")
}

fn still_signature(state: &WallState, key: &str) -> String {
    key.split(',').map(|member| still_background(state, member)).collect::<Vec<_>>().join(" ")
}

pub(super) fn record_still_background(state: &WallState, key: &str) {
    state
        .renderers()
        .set_policy(&format!("{STILL_BACKGROUND_PREFIX}{key}"), &still_signature(state, key));
}

pub(super) fn clear_still_background(state: &WallState, key: &str) {
    state.renderers().set_policy(&format!("{STILL_BACKGROUND_PREFIX}{key}"), "");
}

pub(super) fn still_background_current(state: &WallState, key: &str) -> bool {
    state.renderers().policy(&format!("{STILL_BACKGROUND_PREFIX}{key}"))
        == Some(still_signature(state, key))
}

pub(super) fn still_kept(state: &WallState, key: &str) -> bool {
    state.renderers().has_output_still(key) && still_background_current(state, key)
}
