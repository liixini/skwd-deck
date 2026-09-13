use rusqlite::OptionalExtension;
use serde_json::{Value, json};

use crate::{WallState, config::Config};

pub const ROLE_KEYS: [&str; 9] = [
    "primary",
    "primaryText",
    "tertiary",
    "surface",
    "surfaceText",
    "surfaceVariant",
    "surfaceContainer",
    "background",
    "outline",
];

pub fn valid_palette(palette: &Value) -> bool {
    ROLE_KEYS.iter().all(|key| {
        palette.get(key).and_then(Value::as_str).is_some_and(|color| {
            color.len() == 7
                && color.starts_with('#')
                && color[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
        })
    })
}

pub fn identity(state: &WallState, source: &str) -> Value {
    state.with_db(|db| db.query_row(
        "SELECT key, name, thumb FROM meta WHERE key = ?1 OR thumb = ?1 OR thumb_sm = ?1 OR video_file = ?1 ORDER BY key = ?1 DESC LIMIT 1",
        [source], |row| Ok(json!({"key": row.get::<_, String>(0)?,
            "name": row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            "thumb": row.get::<_, Option<String>>(2)?.unwrap_or_default()})),
    ).optional()).ok().flatten().unwrap_or_else(|| json!({"key": source, "name": source, "thumb": source}))
}

pub fn selected(config: &Config, key: &str, dark: bool) -> Option<Value> {
    if config.theme().policy() != "wallpaper" {
        return None;
    }
    config
        .theme()
        .wallpaper_profiles()
        .iter()
        .find(|profile| {
            profile["key"].as_str() == Some(key) && profile["enabled"].as_bool() == Some(true)
        })?
        .get(if dark { "dark" } else { "light" })
        .filter(|palette| valid_palette(palette))
        .cloned()
}

pub fn palette(state: &WallState, source: &str) -> Option<Value> {
    let config = state.config().clone();
    if config.theme().policy() != "wallpaper" || config.theme().wallpaper_profiles().is_empty() {
        return None;
    }
    let identity = identity(state, source);
    selected(&config, identity["key"].as_str()?, super::resolve_dark(&config, source))
}

pub fn current(state: &WallState) -> anyhow::Result<Value> {
    state
        .theme()
        .applied_theme()
        .ok_or_else(|| anyhow::anyhow!("no applied palette is available yet"))
}

pub fn remember_applied(state: &WallState, source: &str) -> anyhow::Result<()> {
    let config = state.config().clone();
    if config.theme().policy() == "off" {
        return Ok(());
    }
    let mut result = identity(state, source);
    let bytes = std::fs::read(std::path::Path::new(&config.cache_dir()).join("colors.json"))?;
    let mut palette: Value = serde_json::from_slice(&bytes)?;
    for (key, alias) in [
        ("primaryText", "on_primary"),
        ("surfaceText", "on_surface"),
        ("surfaceVariant", "surface_variant"),
        ("surfaceContainer", "surface_container"),
    ] {
        if palette.get(key).is_none()
            && let Some(value) = palette.get(alias).cloned()
        {
            palette[key] = value;
        }
    }
    anyhow::ensure!(valid_palette(&palette), "the applied palette is incomplete");
    result["palette"] = palette;
    if let Ok(bytes) = std::fs::read(super::scheme_path(&config))
        && let Ok(scheme) = serde_json::from_slice::<Value>(&bytes)
    {
        result["scheme"] = scheme;
    }
    result["dark"] = if config.theme().authority() == "dms" {
        result
            .pointer("/scheme/is_dark_mode")
            .cloned()
            .unwrap_or_else(|| json!(super::resolve_dark(&config, source)))
    } else {
        json!(super::resolve_dark(&config, source))
    };
    result["source"] = json!(source);
    let mut app_palette = result["palette"].clone();
    if let Some(scheme) = result.get("scheme") {
        app_palette["_scheme"] = scheme.clone();
    }
    super::apps::apply(&config, &app_palette, result["dark"].as_bool().unwrap_or(true));
    state.theme().set_applied_theme(result);
    Ok(())
}

pub fn apply(state: &WallState, source: &str) -> bool {
    let config = state.config().clone();
    let Some(palette) = palette(state, source) else {
        return super::apply(&config, source);
    };
    let dark = super::resolve_dark(&config, source);
    publish(&config, &palette, dark)
}

pub fn publish(config: &Config, palette: &Value, dark: bool) -> bool {
    let Some(scheme) = crate::material::from_palette(palette, dark, &config.theme().scheme())
    else {
        return false;
    };
    if !publish_document(config, &scheme, dark, true) {
        return false;
    }
    crate::matugen::run_reloads(config);
    true
}

pub fn publish_document(config: &Config, scheme: &Value, dark: bool, render: bool) -> bool {
    let mut scheme = scheme.clone();
    crate::material::select_mode(&mut scheme, dark);
    let Some(mut palette) = crate::material::ui_palette(&scheme) else {
        return false;
    };
    let bytes = palette.to_string();
    for path in [
        config.theme().native_colors_path(),
        std::path::Path::new(&config.cache_dir()).join("colors.json"),
    ] {
        if let Err(error) = crate::paths::atomic_write(&path, bytes.as_bytes()) {
            log::warn!("palette profile: {}: {error}", path.display());
            return false;
        }
    }
    if let Err(error) =
        crate::paths::atomic_write(&super::scheme_path(config), scheme.to_string().as_bytes())
    {
        log::warn!("palette profile scheme: {error}");
        return false;
    }
    crate::theme_provider::publish(config, &scheme);
    if render {
        palette["_scheme"] = scheme;
        palette["_schemeVersion"] = json!(1);
        crate::static_templates::render_integrations(config, &palette, dark);
    }
    true
}

#[cfg(test)]
mod tests;
