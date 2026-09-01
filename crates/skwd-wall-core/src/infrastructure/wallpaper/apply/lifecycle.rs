use crate::domain::wallpaper::is_safe_positional;
use crate::infrastructure::renderers::{HeldRenderer, ReadyWaiter, kill_held_renderer};
use crate::state::WallState;

use super::launch::{RendererLaunchSpec, RendererStartup};

const TRANSITION_SESSION_PAUSE_GRACE: std::time::Duration = std::time::Duration::from_millis(120);

pub(super) fn validate_source(arg: &str) -> anyhow::Result<()> {
    if !is_safe_positional(arg) {
        anyhow::bail!("refusing renderer path/id beginning with '-' (argv injection): {arg}");
    }
    Ok(())
}

pub(super) fn allow_transition_to_finish(state: &WallState, pid: u32, duration_ms: u64) {
    let duration = std::time::Duration::from_millis(duration_ms)
        .saturating_add(TRANSITION_SESSION_PAUSE_GRACE);
    state.renderers_shared().allow_session_rendering_for(pid, duration);
}

pub(super) fn defer_kill_after_swap(
    waiter: Option<ReadyWaiter>,
    old: Vec<HeldRenderer>,
    old_paper: Option<HeldRenderer>,
) {
    if old.is_empty() && old_paper.is_none() {
        return;
    }
    std::thread::spawn(move || {
        match waiter {
            Some(gate) => {
                if gate.wait(std::time::Duration::from_millis(800)) {
                    std::thread::sleep(std::time::Duration::from_millis(120));
                }
            }
            None => std::thread::sleep(std::time::Duration::from_millis(800)),
        }
        for held in old {
            kill_held_renderer(held);
        }
        if let Some(held) = old_paper {
            kill_held_renderer(held);
        }
    });
}

pub(super) fn set_all_video_assignments(state: &WallState, outputs: &[String], path: &str) {
    state.renderers().set_all_assignments(outputs, path);
}

pub(super) fn record_and_dedup(
    state: &WallState,
    outputs: &[String],
    ty: &str,
    path: &str,
    we_id: &str,
    mute: bool,
    volume: u32,
) {
    use std::collections::HashMap;
    let cache = state.config().cache_dir();
    let previous = crate::audio::read_state(&cache);
    crate::audio::record_outputs(
        &cache,
        outputs,
        ty,
        path,
        we_id,
        &HashMap::new(),
        &HashMap::new(),
        mute,
        volume,
    );
    let current = crate::audio::read_state(&cache);
    let reunmute = crate::audio::compute_preserve(&previous, &current);
    if !reunmute.is_empty() {
        state.renderers().send_audio(Some(&reunmute), Some(false), None);
        crate::audio::update_audio(&cache, Some(&reunmute), Some(false), None);
    }
    crate::audio::mute_dedup_losers(state, &cache);
}

pub(super) fn record_static(state: &WallState, resolved: &[(String, String)]) {
    let (cache, default_mute, default_volume) = {
        let config = state.config();
        (config.cache_dir(), config.renderer().mute(), config.renderer().volume())
    };
    let previous = crate::audio::read_state(&cache);
    let mut map = serde_json::Map::new();
    for (output, path) in resolved {
        let (mute, volume) =
            crate::audio::carried_audio(&previous, output, default_mute, default_volume);
        map.insert(
            output.clone(),
            crate::audio::entry(wall_proto::kind::STATIC, path, "", mute, volume),
        );
    }
    crate::audio::write_state(&cache, &serde_json::Value::Object(map));
}

pub(crate) fn spawn_video_paper<'a>(
    state: &'a WallState,
    output: &str,
    args: &[String],
) -> anyhow::Result<RendererStartup<'a>> {
    RendererLaunchSpec::video_for(output, args.to_vec()).spawn(state)
}

pub(crate) fn spawn_native_scene<'a>(
    state: &'a WallState,
    output: &str,
    args: &[String],
) -> anyhow::Result<RendererStartup<'a>> {
    RendererLaunchSpec::native_scene(output, args.to_vec()).spawn(state)
}

pub(crate) fn spawn_base_still<'a>(
    state: &'a WallState,
    output: &str,
    path: &str,
    fill_mode: &str,
) -> anyhow::Result<RendererStartup<'a>> {
    validate_source(path)?;
    RendererLaunchSpec::static_for(output, path, fill_mode).spawn(state)
}
