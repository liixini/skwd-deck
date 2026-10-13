use rusqlite::OptionalExtension;
use serde_json::{Value, json};
use wall_proto::{AppThemeRefresh, AppThemesResult};

use crate::{WallState, config::Config};

const LIGHT_MODE: &str = "light";
const DARK_MODE: &str = "dark";
const APPLIED_DARK_KEY: &str = "dark";
const PALETTE_KEY: &str = "palette";
const SCHEME_KEY: &str = "scheme";
const SCHEME_META_KEY: &str = "_scheme";
pub const ERROR_SEPARATOR: &str = "; ";

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

pub fn app_palette(
    config: &Config,
    name: &str,
    current: &Value,
    dark: bool,
) -> anyhow::Result<Value> {
    if name.is_empty() {
        return Ok(current.clone());
    }
    let palette = super::static_palette_named(config, name, dark)
        .ok_or_else(|| anyhow::anyhow!("Static colour profile '{name}' is unavailable"))?;
    if !valid_palette(&palette) {
        anyhow::bail!("Static colour profile '{name}' is invalid");
    }
    Ok(palette)
}

pub fn refresh_apps(
    state: &WallState,
    request: &AppThemeRefresh,
) -> anyhow::Result<AppThemesResult> {
    if request.integration.is_some() == request.profile.is_some() {
        anyhow::bail!("Choose one app integration or colour profile to refresh");
    }
    let config = state.config().clone();
    let snapshot = state.theme().applied_theme();
    let mut current =
        snapshot.as_ref().and_then(|theme| theme.get(PALETTE_KEY)).cloned().unwrap_or(Value::Null);
    if let Some(scheme) = snapshot.as_ref().and_then(|theme| theme.get(SCHEME_KEY)) {
        current[SCHEME_META_KEY] = scheme.clone();
    }
    let dark = match config.theme().mode().as_str() {
        LIGHT_MODE => false,
        DARK_MODE => true,
        _ => snapshot
            .as_ref()
            .and_then(|theme| theme.get(APPLIED_DARK_KEY).and_then(Value::as_bool))
            .or_else(|| state.theme().source().map(|source| super::resolve_dark(&config, &source)))
            .unwrap_or(true),
    };
    let profile = request.profile.as_deref();
    if profile == Some("") {
        anyhow::bail!("Colour profile name is empty");
    }
    let keep = |integ: &crate::config::Integration| {
        request.integration == Some(integ.index)
            || profile.is_some_and(|name| integ.profile == name)
    };
    let selected: Vec<_> = config.theme().integrations().into_iter().filter(&keep).collect();
    if request.integration.is_some() && selected.is_empty() {
        anyhow::bail!("App integration is unavailable or disabled");
    }
    let mut errors = Vec::new();
    if let Some(name) = profile {
        app_palette(&config, name, &Value::Null, dark)?;
        if let Err(error) = super::apps::refresh_profile(&config, name, dark) {
            errors.push(error.to_string());
        }
    }
    for integ in &selected {
        let result = (|| -> anyhow::Result<()> {
            if integ.template.is_empty() || integ.output.is_empty() {
                anyhow::bail!("App integration has no template or output");
            }
            let output = crate::static_templates::integration_output(&config, &integ.output);
            if crate::static_templates::is_bridge_output(&config, &output)
                || super::apps::protects_output(&output)
            {
                anyhow::bail!("App integration targets a protected colour output");
            }
            if integ.profile.is_empty() && !valid_palette(&current) {
                anyhow::bail!("No applied colour palette is available");
            }
            app_palette(&config, &integ.profile, &current, dark)?;
            let index = integ.index;
            if crate::static_templates::render_integrations_where(
                &config,
                &current,
                dark,
                |entry| entry.index == index,
            ) != 1
            {
                anyhow::bail!("App integration output could not be rendered");
            }
            crate::matugen::run_reloads_where(&config, |entry| entry.index == index);
            Ok(())
        })();
        if let Err(error) = result {
            errors.push(format!("{}: {error:#}", integ.name));
            if profile.is_none() {
                break;
            }
        }
    }
    if !errors.is_empty() {
        anyhow::bail!("{}", errors.join(ERROR_SEPARATOR));
    }
    Ok(super::apps::list(&config))
}

pub fn identity(state: &WallState, source: &str) -> Value {
    state.with_db(|db| db.query_row(
        "SELECT key, name, thumb FROM meta WHERE key = ?1 OR thumb = ?1 OR thumb_sm = ?1 OR video_file = ?1 ORDER BY key = ?1 DESC LIMIT 1",
        [source], |row| Ok(json!({"key": row.get::<_, String>(0)?,
            "name": row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            "thumb": row.get::<_, Option<String>>(2)?.unwrap_or_default()})),
    ).optional()).ok().flatten().unwrap_or_else(|| json!({"key": source, "name": source, "thumb": source}))
}

pub fn configuration(state: &WallState, source: &str) -> Config {
    let mut config = state.config().clone();
    let identity = identity(state, source);
    if let Some(settings) = skwd_config::theme_profile::settings(
        config.theme().wallpaper_profiles(),
        identity["key"].as_str().unwrap_or(source),
    ) {
        config = config.with_overrides(settings);
    }
    config
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
    let config = configuration(state, source);
    palette_for(state, source, &config)
}

fn palette_for(state: &WallState, source: &str, config: &Config) -> Option<Value> {
    if config.theme().policy() != "wallpaper" || config.theme().wallpaper_profiles().is_empty() {
        return None;
    }
    let identity = identity(state, source);
    selected(config, identity["key"].as_str()?, super::resolve_dark(config, source))
}

pub fn current(state: &WallState) -> anyhow::Result<Value> {
    state
        .theme()
        .applied_theme()
        .ok_or_else(|| anyhow::anyhow!("no applied palette is available yet"))
}

pub fn remember_applied(state: &WallState, source: &str, config: &Config) -> anyhow::Result<()> {
    if config.theme().policy() == "off" {
        return Ok(());
    }
    let mut result = identity(state, source);
    let bytes = std::fs::read(std::path::Path::new(&config.cache_dir()).join("colors.json"))?;
    let mut palette: Value = serde_json::from_slice(&bytes)?;
    super::alias_ui_keys(&mut palette);
    anyhow::ensure!(valid_palette(&palette), "the applied palette is incomplete");
    result[PALETTE_KEY] = palette;
    if let Ok(bytes) = std::fs::read(super::scheme_path(config))
        && let Ok(scheme) = serde_json::from_slice::<Value>(&bytes)
    {
        result[SCHEME_KEY] = scheme;
    }
    result["dark"] = if config.theme().authority() == "dms" || config.theme().backend() == "matugen"
    {
        result
            .pointer("/scheme/is_dark_mode")
            .cloned()
            .unwrap_or_else(|| json!(super::resolve_dark(config, source)))
    } else {
        json!(super::resolve_dark(config, source))
    };
    result["source"] = json!(source);
    let mut app_palette = result[PALETTE_KEY].clone();
    if let Some(scheme) = result.get(SCHEME_KEY) {
        app_palette[SCHEME_META_KEY] = scheme.clone();
    }
    super::apps::apply(config, &app_palette, result["dark"].as_bool().unwrap_or(true));
    state.theme().set_applied_theme(result);
    Ok(())
}

pub fn apply(state: &WallState, source: &str, config: &Config) -> bool {
    let Some(palette) = palette_for(state, source, config) else {
        return super::apply(config, source);
    };
    let dark = super::resolve_dark(config, source);
    publish(config, &palette, dark)
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
        palette[SCHEME_META_KEY] = scheme;
        palette["_schemeVersion"] = json!(1);
        crate::static_templates::render_integrations(config, &palette, dark);
    }
    true
}

#[cfg(test)]
mod tests;
