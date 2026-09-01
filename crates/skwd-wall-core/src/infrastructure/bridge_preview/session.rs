use std::path::PathBuf;

use anyhow::Context;
use serde_json::Value;

use crate::config::{Config, Integration};
use crate::state::WallState;

fn is_live_side(config: &Config, integ: &Integration) -> bool {
    !integ.template.is_empty()
        && !integ.output.is_empty()
        && integ.live_preview
        && crate::static_templates::integration_output(config, &integ.output) != bridge_path(config)
}

fn side_snapshots(config: &Config) -> Vec<(PathBuf, Vec<u8>)> {
    config
        .theme()
        .integrations()
        .iter()
        .filter(|integ| is_live_side(config, integ))
        .map(|integ| {
            let path = crate::static_templates::integration_output(config, &integ.output);
            let bytes = std::fs::read(&path).unwrap_or_default();
            (path, bytes)
        })
        .collect()
}

fn preview_side_integrations(config: &Config, palette: &Value, dark: bool) {
    let keep: &dyn Fn(&Integration) -> bool = &|integ: &Integration| is_live_side(config, integ);
    crate::static_templates::render_integrations_where(config, palette, dark, keep);
    crate::matugen::run_reloads_where(config, keep);
}

fn restore_side_integrations(config: &Config, files: &[(PathBuf, Vec<u8>)]) {
    let mut restored = false;
    for (path, bytes) in files {
        if bytes.is_empty() {
            continue;
        }
        match crate::paths::atomic_write(path, bytes) {
            Ok(()) => restored = true,
            Err(err) => log::warn!("shell preview: restore {} failed: {err}", path.display()),
        }
    }
    if restored {
        let keep: &dyn Fn(&Integration) -> bool =
            &|integ: &Integration| is_live_side(config, integ);
        crate::matugen::run_reloads_where(config, keep);
    }
}

fn bridge_path(config: &Config) -> PathBuf {
    PathBuf::from(config.cache_dir()).join("colors.json")
}

fn cache_key(config: &Config, image: &str) -> String {
    format!(
        "bridge\u{0}{}\u{0}{}\u{0}{}\u{0}{}\u{0}{image}",
        config.theme().backend(),
        config.theme().style(),
        config.theme().scheme(),
        config.theme().mode()
    )
}

pub fn arm(state: &WallState) {
    let _serial = state.theme().lock_shell_preview();
    if state.theme().bridge_preview_armed() {
        return;
    }
    let config = state.config();
    let path = bridge_path(&config);
    state.theme().arm_bridge_preview(std::fs::read(&path).unwrap_or_default());
    state.theme().arm_preview_files(side_snapshots(&config));
}

pub fn cached_palette(state: &WallState, image: &str) -> Option<serde_json::Value> {
    let config = state.config().clone();
    let key = cache_key(&config, image);
    if let Some(bytes) = state.theme().shell_palette_cached(&key) {
        return serde_json::from_slice(&bytes).ok();
    }
    let palette = crate::theme::preview_palette(&config, image)?;
    state.theme().cache_shell_palette(key, palette.to_string().into_bytes());
    Some(palette)
}

pub fn preview(state: &WallState, image: &str, generation: u64) -> anyhow::Result<()> {
    let _serial = state.theme().lock_shell_preview();
    if state.theme().shell_preview_generation() != generation {
        return Ok(());
    }
    let config = state.config();
    let path = bridge_path(&config);
    let palette =
        cached_palette(state, image).with_context(|| format!("no preview palette for {image}"))?;
    let dark = crate::theme::resolve_dark(&config, image);
    let bytes = crate::static_templates::render_bridge(&config, &palette, dark)
        .unwrap_or_else(|| palette.to_string())
        .into_bytes();
    if state.theme().shell_preview_generation() != generation {
        return Ok(());
    }
    crate::paths::atomic_write(&path, &bytes)
        .with_context(|| format!("bridge preview: write {} failed", path.display()))?;
    if state.theme().shell_preview_generation() == generation {
        preview_side_integrations(&config, &palette, dark);
    }
    Ok(())
}

pub fn preview_end(state: &WallState) {
    let _serial = state.theme().lock_shell_preview();
    let Some(orig) = state.theme().take_bridge_preview() else {
        log::debug!("bridge preview: end with nothing armed");
        return;
    };
    let config = state.config();
    if !orig.is_empty() {
        let path = bridge_path(&config);
        match crate::paths::atomic_write(&path, &orig) {
            Ok(()) => log::info!("bridge preview: restored the applied palette"),
            Err(err) => log::warn!("bridge preview: restore {} failed: {err}", path.display()),
        }
    }
    restore_side_integrations(&config, &state.theme().take_preview_files());
}

pub fn forget(state: &WallState) {
    let _serial = state.theme().lock_shell_preview();
    state.theme().take_bridge_preview();
    state.theme().take_preview_files();
}

#[path = "tests.rs"]
mod tests;
