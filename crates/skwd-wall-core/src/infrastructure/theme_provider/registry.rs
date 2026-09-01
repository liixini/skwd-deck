use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Map, Value, json};

use crate::config::Config;

pub const PROVIDERS: [&str; 4] = ["caelestia", "dms", "noctalia", "end4"];

const MATERIAL_ROLES: [&str; 49] = [
    "background",
    "error",
    "error_container",
    "inverse_on_surface",
    "inverse_primary",
    "inverse_surface",
    "on_background",
    "on_error",
    "on_error_container",
    "on_primary",
    "on_primary_container",
    "on_primary_fixed",
    "on_primary_fixed_variant",
    "on_secondary",
    "on_secondary_container",
    "on_secondary_fixed",
    "on_secondary_fixed_variant",
    "on_surface",
    "on_surface_variant",
    "on_tertiary",
    "on_tertiary_container",
    "on_tertiary_fixed",
    "on_tertiary_fixed_variant",
    "outline",
    "outline_variant",
    "primary",
    "primary_container",
    "primary_fixed",
    "primary_fixed_dim",
    "scrim",
    "secondary",
    "secondary_container",
    "secondary_fixed",
    "secondary_fixed_dim",
    "shadow",
    "surface",
    "surface_bright",
    "surface_container",
    "surface_container_high",
    "surface_container_highest",
    "surface_container_low",
    "surface_container_lowest",
    "surface_dim",
    "surface_tint",
    "surface_variant",
    "tertiary",
    "tertiary_container",
    "tertiary_fixed",
    "tertiary_fixed_dim",
];

fn state_home() -> PathBuf {
    skwd_config::env("XDG_STATE_HOME")
        .filter(|dir| !dir.is_empty())
        .map_or_else(|| PathBuf::from(skwd_config::home()).join(".local/state"), PathBuf::from)
}

fn cache_home() -> PathBuf {
    skwd_config::env("XDG_CACHE_HOME")
        .filter(|dir| !dir.is_empty())
        .map_or_else(|| PathBuf::from(skwd_config::home()).join(".cache"), PathBuf::from)
}

fn config_home() -> PathBuf {
    skwd_config::env("XDG_CONFIG_HOME")
        .filter(|dir| !dir.is_empty())
        .map_or_else(|| PathBuf::from(skwd_config::home()).join(".config"), PathBuf::from)
}

fn caelestia_bin(config: &Config) -> String {
    let configured = config.theme().caelestia_bin();
    if configured.is_empty() { "caelestia".to_string() } else { configured }
}

fn end4_matugen_config(config: &Config) -> PathBuf {
    config
        .theme()
        .default_matugen_config()
        .map_or_else(|| config_home().join("matugen/config.toml"), PathBuf::from)
}

fn end4_shell_entrypoint() -> PathBuf {
    config_home().join("quickshell/ii/shell.qml")
}

fn end4_contract_available(matugen: bool, matugen_config: &Path, shell_entrypoint: &Path) -> bool {
    matugen && matugen_config.is_file() && shell_entrypoint.is_file()
}

pub fn provider_path(provider: &str) -> Option<PathBuf> {
    match provider {
        "caelestia" => Some(state_home().join("caelestia/scheme.json")),
        "dms" => Some(cache_home().join("DankMaterialShell/dms-colors.json")),
        "noctalia" => {
            let root = skwd_config::env("NOCTALIA_CONFIG_HOME")
                .filter(|dir| !dir.is_empty())
                .map_or_else(config_home, PathBuf::from);
            Some(root.join("noctalia/palettes/skwd-wall.json"))
        }
        "end4" => Some(state_home().join("quickshell/user/generated/colors.json")),
        _ => None,
    }
}

pub fn provider_for_path(path: &Path) -> Option<&'static str> {
    PROVIDERS.into_iter().find(|provider| provider_path(provider).as_deref() == Some(path))
}

pub fn authority_available(config: &Config, provider: &str) -> bool {
    match provider {
        "caelestia" => crate::theme::cli_available(&caelestia_bin(config)),
        "end4" => end4_contract_available(
            crate::theme::cli_available("matugen"),
            &end4_matugen_config(config),
            &end4_shell_entrypoint(),
        ),
        _ => false,
    }
}

fn role(doc: &Value, name: &str, mode: &str) -> Option<String> {
    crate::material::role(doc, name, mode)
}

fn snake_to_camel(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upper = false;
    for ch in name.chars() {
        if ch == '_' {
            upper = true;
        } else if upper {
            out.extend(ch.to_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    out
}

fn mode_map(doc: &Value, mode: &str) -> Option<Map<String, Value>> {
    let mut out = Map::new();
    for name in MATERIAL_ROLES {
        out.insert(name.to_string(), Value::String(role(doc, name, mode)?));
    }
    Some(out)
}

fn end4_payload(doc: &Value) -> Option<Value> {
    Some(Value::Object(mode_map(doc, "default")?))
}

fn dms_payload(doc: &Value) -> Option<Value> {
    Some(json!({
        "colors": {
            "dark": mode_map(doc, "dark")?,
            "light": mode_map(doc, "light")?,
        }
    }))
}

fn terminal_palette(doc: &Value, mode: &str) -> Option<Value> {
    let get = |name: &str| role(doc, name, mode);
    Some(json!({
        "foreground": get("on_surface")?,
        "background": get("surface")?,
        "selectionFg": get("surface")?,
        "selectionBg": get("on_surface")?,
        "cursorText": get("surface")?,
        "cursor": get("on_surface")?,
        "normal": {
            "black": get("surface_container_lowest")?,
            "red": get("error")?,
            "green": get("tertiary")?,
            "yellow": get("tertiary_fixed")?,
            "blue": get("primary")?,
            "magenta": get("secondary")?,
            "cyan": get("secondary_fixed")?,
            "white": get("on_surface_variant")?,
        },
        "bright": {
            "black": get("outline")?,
            "red": get("error_container")?,
            "green": get("tertiary_fixed")?,
            "yellow": get("tertiary_fixed_dim")?,
            "blue": get("primary_fixed")?,
            "magenta": get("secondary_fixed")?,
            "cyan": get("secondary_fixed_dim")?,
            "white": get("on_surface")?,
        }
    }))
}

fn noctalia_mode(doc: &Value, mode: &str) -> Option<Value> {
    let get = |name: &str| role(doc, name, mode);
    Some(json!({
        "mPrimary": get("primary")?,
        "mOnPrimary": get("on_primary")?,
        "mSecondary": get("secondary")?,
        "mOnSecondary": get("on_secondary")?,
        "mTertiary": get("tertiary")?,
        "mOnTertiary": get("on_tertiary")?,
        "mError": get("error")?,
        "mOnError": get("on_error")?,
        "mSurface": get("surface")?,
        "mOnSurface": get("on_surface")?,
        "mSurfaceVariant": get("surface_variant")?,
        "mOnSurfaceVariant": get("on_surface_variant")?,
        "mOutline": get("outline")?,
        "mShadow": get("shadow")?,
        "mHover": get("tertiary")?,
        "mOnHover": get("on_tertiary")?,
        "terminal": terminal_palette(doc, mode)?,
    }))
}

fn noctalia_payload(doc: &Value) -> Option<Value> {
    Some(json!({
        "dark": noctalia_mode(doc, "dark")?,
        "light": noctalia_mode(doc, "light")?,
    }))
}

fn caelestia_payload(path: &Path, doc: &Value, scheme: &str) -> Option<Value> {
    let bytes = std::fs::read(path).ok()?;
    let mut current: Value = serde_json::from_slice(&bytes).ok()?;
    let root = current.as_object_mut()?;
    let colours =
        root.entry("colours").or_insert_with(|| Value::Object(Map::new())).as_object_mut()?;
    for name in MATERIAL_ROLES {
        let hex = role(doc, name, "default")?;
        colours
            .insert(snake_to_camel(name), Value::String(hex.trim_start_matches('#').to_string()));
    }
    root.insert("name".to_string(), Value::String("skwd-wall".to_string()));
    root.insert("flavour".to_string(), Value::String("default".to_string()));
    root.insert("mode".to_string(), Value::String(doc.get("mode")?.as_str()?.to_string()));
    root.insert("variant".to_string(), Value::String(scheme.replace('-', "")));
    Some(current)
}

fn payload(provider: &str, path: &Path, doc: &Value, scheme: &str) -> Option<Value> {
    match provider {
        "caelestia" => caelestia_payload(path, doc, scheme),
        "dms" => dms_payload(doc),
        "noctalia" => noctalia_payload(doc),
        "end4" => end4_payload(doc),
        _ => None,
    }
}

fn origin_path(config: &Config, provider: &str) -> PathBuf {
    PathBuf::from(config.cache_dir()).join("theme-origins").join(format!("{provider}.json"))
}

fn write_payload(
    config: &Config,
    provider: &str,
    path: &Path,
    bytes: &[u8],
) -> std::io::Result<bool> {
    if std::fs::read(path).is_ok_and(|current| current == bytes) {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::paths::atomic_write(path, bytes)?;
    let marker = origin_path(config, provider);
    if let Some(parent) = marker.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::paths::atomic_write(&marker, bytes)?;
    Ok(true)
}

fn activate_noctalia(config: &Config) {
    let mut command = Command::new(crate::noctalia::bin(config));
    command
        .args(["msg", "color-scheme-set", "custom", "skwd-wall"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if !crate::proc::spawn_reaped(&mut command, "noctalia target activate") {
        log::warn!("theme provider noctalia: palette written but activation failed");
    }
}

pub fn publish(config: &Config, doc: &Value) {
    if config.theme().authority() != "skwd" {
        return;
    }
    for provider in config.theme().targets() {
        let Some(path) = provider_path(&provider) else {
            continue;
        };
        if provider == "caelestia" && !path.is_file() {
            log::warn!(
                "theme provider caelestia: {} is missing; run Caelestia once before enabling the target",
                path.display()
            );
            continue;
        }
        let Some(value) = payload(&provider, &path, doc, &config.theme().scheme()) else {
            log::warn!("theme provider {provider}: could not encode the canonical palette");
            continue;
        };
        let Ok(bytes) = serde_json::to_vec_pretty(&value) else {
            log::warn!("theme provider {provider}: JSON encoding failed");
            continue;
        };
        match write_payload(config, &provider, &path, &bytes) {
            Ok(true) => {
                log::info!("theme provider {provider}: wrote {}", path.display());
                if provider == "noctalia" {
                    activate_noctalia(config);
                }
            }
            Ok(false) => log::debug!("theme provider {provider}: palette unchanged"),
            Err(err) => {
                log::warn!("theme provider {provider}: write {} failed: {err}", path.display());
            }
        }
    }
}

pub fn is_published_echo(config: &Config, provider: &str, bytes: &[u8]) -> bool {
    std::fs::read(origin_path(config, provider)).is_ok_and(|published| published == bytes)
}

fn with_hash(value: &str) -> Option<String> {
    let digits = value.trim().trim_start_matches('#');
    (digits.len() == 6 && digits.chars().all(|ch| ch.is_ascii_hexdigit()))
        .then(|| format!("#{}", digits.to_ascii_lowercase()))
}

fn canonical_palette(mut get: impl FnMut(&str) -> Option<String>) -> Option<Value> {
    let mut pick = |name: &str| with_hash(&get(name)?);
    Some(json!({
        "primary": pick("primary")?,
        "primaryText": pick("on_primary")?,
        "primaryContainer": pick("primary_container")?,
        "primaryContainerText": pick("on_primary_container")?,
        "secondary": pick("secondary")?,
        "secondaryText": pick("on_secondary")?,
        "secondaryContainer": pick("secondary_container")?,
        "secondaryContainerText": pick("on_secondary_container")?,
        "tertiary": pick("tertiary")?,
        "tertiaryText": pick("on_tertiary")?,
        "tertiaryContainer": pick("tertiary_container")?,
        "tertiaryContainerText": pick("on_tertiary_container")?,
        "background": pick("background")?,
        "backgroundText": pick("on_background")?,
        "surface": pick("surface")?,
        "surfaceText": pick("on_surface")?,
        "surfaceVariant": pick("surface_variant")?,
        "surfaceVariantText": pick("on_surface_variant")?,
        "surfaceContainer": pick("surface_container")?,
        "outline": pick("outline")?,
        "shadow": pick("shadow")?,
        "inverseSurface": pick("inverse_surface")?,
        "inverseSurfaceText": pick("inverse_on_surface")?,
        "inversePrimary": pick("inverse_primary")?,
        "error": pick("error")?,
        "errorText": pick("on_error")?,
        "errorContainer": pick("error_container")?,
        "errorContainerText": pick("on_error_container")?,
        "onPrimary": pick("on_primary")?,
    }))
}

pub fn normalize(provider: &str, value: &Value, dark: bool) -> Option<Value> {
    match provider {
        "caelestia" => {
            let colours = value.get("colours")?;
            canonical_palette(|name| {
                colours.get(snake_to_camel(name)).and_then(Value::as_str).map(str::to_string)
            })
        }
        "dms" => {
            let colors = value.get("colors")?.get(if dark { "dark" } else { "light" })?;
            canonical_palette(|name| colors.get(name).and_then(Value::as_str).map(str::to_string))
        }
        "noctalia" => {
            let mode = value.get(if dark { "dark" } else { "light" })?;
            canonical_palette(|name| {
                let key = match name {
                    "primary" | "primary_container" | "inverse_primary" => "mPrimary",
                    "on_primary" | "on_primary_container" => "mOnPrimary",
                    "secondary" | "secondary_container" => "mSecondary",
                    "on_secondary" | "on_secondary_container" => "mOnSecondary",
                    "tertiary" | "tertiary_container" => "mTertiary",
                    "on_tertiary" | "on_tertiary_container" => "mOnTertiary",
                    "error" | "error_container" => "mError",
                    "on_error" | "on_error_container" => "mOnError",
                    "surface" | "background" | "surface_container" | "inverse_on_surface" => {
                        "mSurface"
                    }
                    "on_surface" | "on_background" | "inverse_surface" => "mOnSurface",
                    "surface_variant" => "mSurfaceVariant",
                    "on_surface_variant" => "mOnSurfaceVariant",
                    "outline" => "mOutline",
                    "shadow" => "mShadow",
                    _ => return None,
                };
                mode.get(key).and_then(Value::as_str).map(str::to_string)
            })
        }
        "end4" => {
            canonical_palette(|name| value.get(name).and_then(Value::as_str).map(str::to_string))
        }
        _ => None,
    }
}

pub fn import(config: &Config, provider: &str, dark: bool) -> bool {
    let Some(path) = provider_path(provider) else {
        return false;
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return false;
    };
    if is_published_echo(config, provider, &bytes) {
        log::debug!("theme provider {provider}: ignored our own echoed palette");
        return false;
    }
    let Some(palette) = serde_json::from_slice::<Value>(&bytes)
        .ok()
        .and_then(|value| normalize(provider, &value, dark))
    else {
        log::warn!("theme provider {provider}: {} is not a compatible palette", path.display());
        return false;
    };
    let output = PathBuf::from(config.cache_dir()).join("colors.json");
    let Ok(payload) = serde_json::to_vec_pretty(&palette) else {
        return false;
    };
    if std::fs::read(&output).is_ok_and(|current| current == payload) {
        return false;
    }
    match crate::paths::atomic_write(&output, &payload) {
        Ok(()) => {
            log::info!("theme provider {provider}: imported {}", path.display());
            true
        }
        Err(err) => {
            log::warn!(
                "theme provider {provider}: import write {} failed: {err}",
                output.display()
            );
            false
        }
    }
}

fn run_authority(config: &Config, provider: &str, image: &str, dark: bool) -> bool {
    let mut command = match provider {
        "caelestia" => {
            let binary = caelestia_bin(config);
            let wallpaper = Command::new(&binary)
                .args(["wallpaper", "-f", image])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            if !wallpaper.is_ok_and(|status| status.success()) {
                log::warn!("theme authority caelestia: wallpaper command failed");
                return false;
            }
            let mut command = Command::new(binary);
            command.args([
                "scheme",
                "set",
                "-n",
                "dynamic",
                "-m",
                if dark { "dark" } else { "light" },
            ]);
            command
        }
        "end4" => {
            let mut command = Command::new("matugen");
            command
                .arg("-c")
                .arg(end4_matugen_config(config))
                .arg("image")
                .arg("-m")
                .arg(if dark { "dark" } else { "light" })
                .arg("--source-color-index")
                .arg(config.theme().matugen_color_index().to_string())
                .arg(image);
            command
        }
        _ => return false,
    };
    match command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status() {
        Ok(status) if status.success() => true,
        Ok(status) => {
            log::warn!("theme authority {provider}: exited {status}");
            false
        }
        Err(err) => {
            log::warn!("theme authority {provider}: launch failed: {err}");
            false
        }
    }
}

fn readable_palette(provider: &str, dark: bool) -> bool {
    provider_path(provider)
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|value| normalize(provider, &value, dark))
        .is_some()
}

pub fn apply_authority(config: &Config, provider: &str, image: &str, dark: bool) -> bool {
    if !run_authority(config, provider, image, dark) {
        return false;
    }
    let changed = import(config, provider, dark);
    if changed || readable_palette(provider, dark) {
        true
    } else {
        log::warn!("theme authority {provider}: produced no compatible palette");
        false
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
