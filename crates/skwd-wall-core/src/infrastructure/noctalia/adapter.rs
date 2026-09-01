use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::Context;

use crate::config::Config;
use crate::state::WallState;

pub const PREVIEW_SCHEME: &str = "skwd-hover";
const APPLIED_SCHEME: &str = "skwd-wall";
const FALLBACK_SCHEME: &str = "m3-content";

pub fn bin_from(configured: &str, local_candidate: &std::path::Path) -> String {
    crate::shell_adapter::bin_from(configured, local_candidate, "noctalia")
}

pub fn bin(config: &Config) -> String {
    bin_from(
        &config.theme().noctalia_bin(),
        &PathBuf::from(skwd_config::home()).join(".local/bin/noctalia"),
    )
}

fn msg_capture(config: &Config, args: &[&str]) -> Option<String> {
    let out = Command::new(bin(config))
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn msg_fire(config: &Config, args: &[String], what: &str) {
    let mut cmd = Command::new(bin(config));
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    if !crate::proc::spawn_reaped(&mut cmd, what) {
        log::warn!("noctalia {what}: spawn failed");
    }
}

const MSG_TIMEOUT_MS: u64 = 2000;

fn msg_wait(config: &Config, args: &[String], what: &str) -> bool {
    let mut child = match Command::new(bin(config))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            log::warn!("noctalia {what}: spawn failed: {err}");
            return false;
        }
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(MSG_TIMEOUT_MS);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    log::warn!("noctalia {what}: exited {status}");
                }
                return status.success();
            }
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    log::warn!("noctalia {what}: no response in {MSG_TIMEOUT_MS}ms, giving up");
                    let _ = child.kill();
                    let _ = child.wait();
                    return false;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(err) => {
                log::warn!("noctalia {what}: wait failed: {err}");
                return false;
            }
        }
    }
}

fn remove_preview_palette() {
    let path = palettes_dir().join(format!("{PREVIEW_SCHEME}.json"));
    if let Err(err) = std::fs::remove_file(&path)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        log::warn!("noctalia: could not remove {}: {err}", path.display());
    }
}

pub fn parse_scheme(line: &str) -> String {
    let mut it = line.split_whitespace();
    match (it.next(), it.next()) {
        (Some("wallpaper"), Some(name)) => name.to_string(),
        _ => FALLBACK_SCHEME.to_string(),
    }
}

pub fn theme_gen_args(image: &str, scheme: &str, variant: &str, pure_black: bool) -> Vec<String> {
    let mut args = vec![
        "theme".to_string(),
        image.to_string(),
        "--scheme".to_string(),
        scheme.to_string(),
        variant.to_string(),
    ];
    if pure_black {
        args.push("--pure-black".to_string());
    }
    args
}

fn generate_palette(config: &Config, image: &str, scheme: &str, variant: &str) -> Option<Vec<u8>> {
    let out = Command::new(bin(config))
        .args(theme_gen_args(image, scheme, variant, config.theme().noctalia_pure_black()))
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() || serde_json::from_slice::<serde_json::Value>(&out.stdout).is_err() {
        return None;
    }
    Some(out.stdout)
}

pub fn preview_palette(config: &Config, image: &str, dark: bool) -> Option<serde_json::Value> {
    let scheme = config
        .theme()
        .noctalia_scheme_override()
        .unwrap_or_else(|| active_gen_scheme(config, None));
    let variant = if dark { "--dark" } else { "--light" };
    let bytes = generate_palette(config, image, &scheme, variant)?;
    serde_json::from_slice(&bytes).ok()
}

fn active_gen_scheme(config: &Config, stored: Option<&(String, String)>) -> String {
    match stored {
        Some((source, name)) => parse_scheme(&format!("{source} {name}")),
        None => msg_capture(config, &["msg", "color-scheme-get"])
            .map_or_else(|| FALLBACK_SCHEME.to_string(), |line| parse_scheme(&line)),
    }
}

pub fn write_bridge_palette(config: &Config, image: &str, dark: bool) -> bool {
    let scheme = config
        .theme()
        .noctalia_scheme_override()
        .unwrap_or_else(|| active_gen_scheme(config, None));
    let Some(json) = generate_palette(config, image, &scheme, "--both") else {
        log::warn!("noctalia palette bridge: generation failed for {image}");
        return false;
    };
    let Ok(tokens) = serde_json::from_slice::<serde_json::Value>(&json) else {
        log::warn!("noctalia palette bridge: generated palette is not valid JSON for {image}");
        return false;
    };
    let mode = if dark { "dark" } else { "light" };
    let Some(colors) = tokens.get(mode) else {
        log::warn!("noctalia palette bridge: generated palette is missing {mode} colours");
        return false;
    };
    let path = PathBuf::from(config.cache_dir()).join("colors.json");
    if let Err(err) = crate::paths::atomic_write(&path, colors.to_string().as_bytes()) {
        log::warn!("noctalia palette bridge: write {} failed: {err}", path.display());
        return false;
    }
    let Some(custom) = cli_tokens_to_custom_palette(&tokens) else {
        log::warn!("noctalia palette bridge: could not encode shell palette for {image}");
        return false;
    };
    let palette_path = palettes_dir().join(format!("{APPLIED_SCHEME}.json"));
    if let Some(dir) = palette_path.parent()
        && let Err(err) = std::fs::create_dir_all(dir)
    {
        log::warn!("noctalia palette bridge: create {} failed: {err}", dir.display());
        return false;
    }
    if let Err(err) = crate::paths::atomic_write(&palette_path, custom.to_string().as_bytes()) {
        log::warn!("noctalia palette bridge: write {} failed: {err}", palette_path.display());
        return false;
    }
    msg_fire(
        config,
        &restore_args(("custom".to_string(), APPLIED_SCHEME.to_string())),
        "applied scheme set",
    );
    log::info!("noctalia palette bridge: wrote {} and {}", path.display(), palette_path.display());
    true
}

#[allow(clippy::match_same_arms)]
pub fn palettes_dir_from(
    noctalia_home: Option<&str>,
    xdg_config: Option<&str>,
    home: &str,
) -> PathBuf {
    let base = match (noctalia_home, xdg_config) {
        (Some(dir), _) if !dir.is_empty() => PathBuf::from(dir),
        (_, Some(dir)) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(home).join(".config"),
    };
    base.join("noctalia").join("palettes")
}

fn palettes_dir() -> PathBuf {
    palettes_dir_from(
        skwd_config::env("NOCTALIA_CONFIG_HOME").as_deref(),
        skwd_config::env("XDG_CONFIG_HOME").as_deref(),
        &skwd_config::home(),
    )
}

fn preview_orig_path(config: &Config) -> PathBuf {
    PathBuf::from(config.cache_dir()).join("noctalia-preview-orig.json")
}

fn mode_to_custom(mode: &serde_json::Value) -> serde_json::Value {
    let get = |key: &str| mode.get(key).cloned().unwrap_or(serde_json::Value::Null);
    let ansi = |prefix: &str| {
        serde_json::json!({
            "black": get(&format!("terminal_{prefix}_black")),
            "red": get(&format!("terminal_{prefix}_red")),
            "green": get(&format!("terminal_{prefix}_green")),
            "yellow": get(&format!("terminal_{prefix}_yellow")),
            "blue": get(&format!("terminal_{prefix}_blue")),
            "magenta": get(&format!("terminal_{prefix}_magenta")),
            "cyan": get(&format!("terminal_{prefix}_cyan")),
            "white": get(&format!("terminal_{prefix}_white")),
        })
    };
    serde_json::json!({
        "mPrimary": get("primary"),
        "mOnPrimary": get("on_primary"),
        "mSecondary": get("secondary"),
        "mOnSecondary": get("on_secondary"),
        "mTertiary": get("tertiary"),
        "mOnTertiary": get("on_tertiary"),
        "mError": get("error"),
        "mOnError": get("on_error"),
        "mSurface": get("surface"),
        "mOnSurface": get("on_surface"),
        "mSurfaceVariant": get("surface_variant"),
        "mOnSurfaceVariant": get("on_surface_variant"),
        "mOutline": get("outline"),
        "mShadow": get("shadow"),
        "mHover": get("tertiary"),
        "mOnHover": get("on_tertiary"),
        "terminal": {
            "foreground": get("terminal_foreground"),
            "background": get("terminal_background"),
            "selectionFg": get("terminal_selection_fg"),
            "selectionBg": get("terminal_selection_bg"),
            "cursorText": get("terminal_cursor_text"),
            "cursor": get("terminal_cursor"),
            "normal": ansi("normal"),
            "bright": ansi("bright"),
        }
    })
}

pub fn cli_tokens_to_custom_palette(cli: &serde_json::Value) -> Option<serde_json::Value> {
    let dark = cli.get("dark")?;
    let light = cli.get("light")?;
    if !dark.is_object() || !light.is_object() {
        return None;
    }
    Some(serde_json::json!({
        "dark": mode_to_custom(dark),
        "light": mode_to_custom(light),
    }))
}

pub fn preview(state: &WallState, image: &str, generation: u64) -> anyhow::Result<()> {
    let _serial = state.theme().lock_shell_preview();
    if state.theme().shell_preview_generation() != generation {
        return Ok(());
    }
    let config = state.config().clone();
    let stored = state.theme().noctalia_preview_orig();
    let scheme = active_gen_scheme(&config, stored.as_ref());
    let key = format!("{scheme}\u{0}{image}");
    let palette = if let Some(bytes) = state.theme().shell_palette_cached(&key) {
        bytes
    } else {
        let json = generate_palette(&config, image, &scheme, "--both")
            .with_context(|| format!("noctalia preview palette generation failed for {image}"))?;
        let tokens: serde_json::Value = serde_json::from_slice(&json)?;
        let bytes = cli_tokens_to_custom_palette(&tokens)
            .context("noctalia theme CLI output missing dark/light token maps")?
            .to_string()
            .into_bytes();
        state.theme().cache_shell_palette(key, bytes.clone());
        bytes
    };
    if state.theme().shell_preview_generation() != generation {
        return Ok(());
    }
    let dir = palettes_dir();
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(format!("{PREVIEW_SCHEME}.json")), &palette)?;
    if stored.is_none() {
        let line = msg_capture(&config, &["msg", "color-scheme-get"]).unwrap_or_default();
        let orig = sanitize_orig_scheme(&line);
        let _ = std::fs::write(
            preview_orig_path(&config),
            serde_json::json!({"source": orig.0, "name": orig.1}).to_string(),
        );
        state.theme().set_noctalia_preview_orig(orig);
        msg_fire(
            &config,
            &restore_args(("custom".to_string(), PREVIEW_SCHEME.to_string())),
            "preview scheme set",
        );
    } else {
        msg_fire(&config, &["msg".to_string(), "config-reload".to_string()], "preview reload");
    }
    Ok(())
}

pub fn sanitize_orig_scheme(line: &str) -> (String, String) {
    let mut it = line.split_whitespace();
    let source = it.next().unwrap_or("wallpaper").to_string();
    let name = it.next().unwrap_or(FALLBACK_SCHEME).to_string();
    if name == PREVIEW_SCHEME {
        (String::from("wallpaper"), FALLBACK_SCHEME.to_string())
    } else {
        (source, name)
    }
}

pub fn restore_args(orig: (String, String)) -> Vec<String> {
    vec!["msg".to_string(), "color-scheme-set".to_string(), orig.0, orig.1]
}

pub fn preview_end(state: &WallState) {
    let _serial = state.theme().lock_shell_preview();
    let Some(orig) = state.theme().take_noctalia_preview_orig() else {
        return;
    };
    let config = state.config().clone();
    if !msg_wait(&config, &restore_args(orig.clone()), "preview restore") {
        log::warn!(
            "noctalia: restore to {}/{} failed, keeping the recovery marker so a later run can retry",
            orig.0,
            orig.1
        );
        state.theme().set_noctalia_preview_orig(orig);
        return;
    }
    let _ = std::fs::remove_file(preview_orig_path(&config));
    remove_preview_palette();
}

pub fn restore_stale_preview(config: &Config) {
    let path = preview_orig_path(config);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) else {
        log::warn!(
            "noctalia: recovery marker {} is unreadable, leaving it in place rather than guessing a scheme",
            path.display()
        );
        return;
    };
    let source = val.get("source").and_then(serde_json::Value::as_str).unwrap_or("wallpaper");
    let name = val.get("name").and_then(serde_json::Value::as_str).unwrap_or(FALLBACK_SCHEME);
    log::info!("noctalia: restoring colour scheme left by an interrupted preview");
    if msg_wait(config, &restore_args((source.to_string(), name.to_string())), "stale restore") {
        let _ = std::fs::remove_file(&path);
        remove_preview_palette();
    } else {
        log::warn!(
            "noctalia: stale restore failed (shell may not be up yet), keeping the marker for the next start"
        );
    }
}

#[path = "tests.rs"]
mod tests;
