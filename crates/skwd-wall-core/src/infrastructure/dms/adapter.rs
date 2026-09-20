use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::Context;
use serde_json::Value;

use crate::config::Config;
use crate::state::WallState;

const FALLBACK_SCHEME: &str = "scheme-tonal-spot";

pub fn colors_path_from(xdg_cache: Option<&str>, home: &str) -> PathBuf {
    let base = match xdg_cache {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(home).join(".cache"),
    };
    base.join("DankMaterialShell").join("dms-colors.json")
}

fn colors_path() -> PathBuf {
    colors_path_from(skwd_config::env("XDG_CACHE_HOME").as_deref(), &skwd_config::home())
}

pub fn settings_path_from(xdg_config: Option<&str>, home: &str) -> PathBuf {
    let base = match xdg_config {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(home).join(".config"),
    };
    base.join("DankMaterialShell").join("settings.json")
}

pub fn scheme_from_settings(settings: Option<&Value>) -> String {
    settings
        .and_then(|val| val.get("matugenScheme"))
        .and_then(Value::as_str)
        .filter(|scheme| !scheme.is_empty())
        .map_or_else(|| FALLBACK_SCHEME.to_string(), ToString::to_string)
}

fn dms_scheme() -> String {
    let path =
        settings_path_from(skwd_config::env("XDG_CONFIG_HOME").as_deref(), &skwd_config::home());
    let settings = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    scheme_from_settings(settings.as_ref())
}

pub fn matugen_gen_args(image: &str, scheme: &str, index: u32) -> Vec<String> {
    vec![
        "image".to_string(),
        image.to_string(),
        "--dry-run".to_string(),
        "-j".to_string(),
        "hex".to_string(),
        "-t".to_string(),
        scheme.to_string(),
        "--source-color-index".to_string(),
        index.to_string(),
    ]
}

fn generate(image: &str, scheme: &str, index: u32) -> Option<Value> {
    let out = Command::new("matugen")
        .args(matugen_gen_args(image, scheme, index))
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    serde_json::from_slice::<Value>(&out.stdout).ok()
}

pub fn preview_palette(config: &Config, image: &str, dark: bool) -> Option<Value> {
    let scheme = config.theme().matugen_scheme_override().unwrap_or_else(dms_scheme);
    let tokens = generate(image, &scheme, config.theme().matugen_color_index())?;
    flat_mode_colors(&tokens, dark)
}

fn mode_map(tokens: &Value, mode: &str) -> Option<Value> {
    let colors = tokens.get("colors")?.as_object()?;
    let mut out = serde_json::Map::new();
    for (name, node) in colors {
        if let Some(hex) =
            node.get(mode).and_then(|mnode| mnode.get("color")).and_then(Value::as_str)
        {
            out.insert(name.clone(), Value::String(hex.to_string()));
        }
    }
    (!out.is_empty()).then_some(Value::Object(out))
}

pub fn dank_colors_json(tokens: &Value) -> Option<Value> {
    let dark = mode_map(tokens, "dark")?;
    let light = mode_map(tokens, "light")?;
    Some(serde_json::json!({ "colors": { "dark": dark, "light": light } }))
}

pub fn flat_mode_colors(tokens: &Value, dark: bool) -> Option<Value> {
    mode_map(tokens, if dark { "dark" } else { "light" })
}

pub fn imported_shell_colors(shell_palette: &Value, dark: bool) -> Option<Value> {
    let mode = if dark { "dark" } else { "light" };
    shell_palette
        .get("colors")?
        .get(mode)
        .cloned()
        .filter(|colors| colors.as_object().is_some_and(|roles| !roles.is_empty()))
}

fn write_shell_colors(payload: &[u8]) -> std::io::Result<()> {
    let path = colors_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    crate::paths::atomic_write(&path, payload)
}

pub fn write_bridge_palette(config: &Config, image: &str, dark: bool) -> bool {
    if let Err(error) = super::generation::apply(config, image, dark) {
        log::warn!("dms theme generation failed: {error:#}");
        return false;
    }
    let Some(document) = std::fs::read(colors_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|palette| shell_document(&palette, dark))
    else {
        log::warn!("dms theme generation did not produce a complete palette");
        return false;
    };
    crate::theme::profiles::publish_document(
        config,
        &document,
        document["is_dark_mode"].as_bool().unwrap_or(dark),
        true,
    )
}

pub fn shell_document(palette: &Value, dark: bool) -> Option<Value> {
    let dark = match palette.get("mode").and_then(Value::as_str) {
        Some("dark") => true,
        Some("light") => false,
        _ => dark,
    };
    let mut document = crate::material::document_from_modes(palette.get("colors")?, dark)?;
    if let Some(dank16) = palette.get("dank16") {
        document["dank16"] = dank16.clone();
    }
    Some(document)
}

fn preview_orig_path(config: &Config) -> PathBuf {
    PathBuf::from(config.cache_dir()).join("dms-preview-orig.json")
}

pub fn preview(state: &WallState, image: &str, generation: u64) -> anyhow::Result<()> {
    let _serial = state.theme().lock_shell_preview();
    if state.theme().shell_preview_generation() != generation {
        return Ok(());
    }
    let config = crate::theme::profiles::configuration(state, image);
    let shell_colors = colors_path();
    if !shell_colors.is_file() {
        anyhow::bail!(
            "dms preview: {} missing - is DMS running in Auto theme?",
            shell_colors.display()
        );
    }
    let scheme = config.theme().matugen_scheme_override().unwrap_or_else(dms_scheme);
    let index = config.theme().matugen_color_index();
    let key = format!("dms\u{0}{scheme}\u{0}{index}\u{0}{image}");
    let payload = if let Some(bytes) = state.theme().shell_palette_cached(&key) {
        bytes
    } else {
        let tokens = generate(image, &scheme, index)
            .with_context(|| format!("dms preview palette generation failed for {image}"))?;
        let bytes = dank_colors_json(&tokens)
            .context("matugen output missing dark/light colors")?
            .to_string()
            .into_bytes();
        state.theme().cache_shell_palette(key, bytes.clone());
        bytes
    };
    if state.theme().shell_preview_generation() != generation {
        return Ok(());
    }
    let backup = preview_orig_path(&config);
    if !backup.exists() {
        std::fs::copy(&shell_colors, &backup)?;
    }
    write_shell_colors(&payload)?;
    Ok(())
}

pub fn preview_end(state: &WallState) {
    let _serial = state.theme().lock_shell_preview();
    restore_from_backup(&state.config());
}

fn restore_from_backup(config: &Config) {
    let backup = preview_orig_path(config);
    let Ok(orig) = std::fs::read(&backup) else {
        return;
    };
    if let Err(err) = write_shell_colors(&orig) {
        log::warn!("dms preview restore failed: {err}");
        return;
    }
    let _ = std::fs::remove_file(&backup);
}

pub fn restore_stale_preview(config: &Config) {
    if preview_orig_path(config).exists() {
        log::info!("dms: restoring colours left by an interrupted preview");
        restore_from_backup(config);
    }
}

#[path = "tests.rs"]
mod tests;
