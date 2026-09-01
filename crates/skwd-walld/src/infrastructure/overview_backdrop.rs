use std::path::Path;
use std::process::{Command, Stdio};

use skwd_wall_core::config::Config;

const NAMESPACE: &str = "skwd-paper-backdrop";

fn is_niri() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|desk| desk.to_lowercase().contains("niri"))
}

fn parse_backdrop_source(json_text: &str) -> Option<String> {
    let val: serde_json::Value = serde_json::from_str(json_text).ok()?;
    let thumb = val.get("thumb").and_then(|node| node.as_str()).filter(|text| !text.is_empty());
    let path = val.get("path").and_then(|node| node.as_str()).filter(|text| !text.is_empty());
    thumb.or(path).map(String::from)
}

fn pick_backdrop_source(follow: bool, fixed: &str, last_json: &str) -> Option<String> {
    if !follow {
        let fixed = fixed.trim();
        if !fixed.is_empty() {
            return Some(fixed.to_string());
        }
    }
    parse_backdrop_source(last_json)
}

pub fn resolve_source_from_disk(config: &Config) -> Option<String> {
    let path = skwd_wall_core::paths::cache_dir().join("last-wallpaper.json");
    let last = std::fs::read_to_string(&path).unwrap_or_default();
    pick_backdrop_source(
        config.niri_backdrop_follow_wallpaper(),
        &config.niri_backdrop_source(),
        &last,
    )
    .filter(|src| Path::new(src).exists())
}

fn themed_source(config: &Config, source: &str) -> String {
    if !config.niri_backdrop_auto_theme() {
        return source.to_string();
    }
    let name = config.niri_backdrop_theme();
    let theme = if name.trim().is_empty() { "Catppuccin" } else { name.trim() };
    let out = skwd_wall_core::paths::cache_dir().join("overview-themed.png");
    let params = serde_json::json!({ "theme": theme });
    match crate::infrastructure::effects_preview::effects_render(
        source, "theme", &params, &out, 0, false,
    ) {
        Ok(written) => {
            log::info!("overview-backdrop: themed with '{theme}'");
            written.to_string_lossy().into_owned()
        }
        Err(err) => {
            log::warn!("overview-backdrop: theme '{theme}' failed: {err}");
            source.to_string()
        }
    }
}

fn kill_args() -> [String; 3] {
    ["-f".into(), "--".into(), format!("--namespace {NAMESPACE}")]
}

static CHILD: std::sync::Mutex<Option<std::process::Child>> = std::sync::Mutex::new(None);

fn kill_existing() {
    if let Some(mut child) = CHILD.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take()
    {
        let _ = child.kill();
        let _ = child.wait();
    }
    let _ = Command::new("pkill")
        .args(kill_args())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

pub fn refresh(config: &Config, source: &str) {
    if !is_niri() || !config.niri_overview_backdrop() {
        kill_existing();
        return;
    }
    if source.is_empty() || !Path::new(source).exists() {
        log::warn!("overview-backdrop: no valid source ({source})");
        return;
    }
    kill_existing();
    let render_source = themed_source(config, source);
    let bin = config.renderer().still_bin();
    let mut args: Vec<String> =
        vec!["*".into(), render_source, "--namespace".into(), NAMESPACE.into()];
    if config.niri_backdrop_blur_enabled() {
        args.push("--blur".into());
        args.push(config.niri_backdrop_blur().to_string());
    }
    let dim = config.niri_backdrop_dim();
    if dim > 0 {
        args.push("--dim".into());
        args.push(dim.to_string());
    }
    log::info!("overview-backdrop: spawn {bin} {}", args.join(" "));
    match Command::new(&bin)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => {
            *CHILD.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(child);
        }
        Err(err) => log::warn!("overview-backdrop: spawn failed: {err}"),
    }
}

pub fn on_apply(config: &Config) {
    if !config.niri_backdrop_follow_wallpaper() {
        return;
    }
    if let Some(src) = resolve_source_from_disk(config) {
        refresh(config, &src);
    }
}

pub fn refresh_from_disk(config: &Config) {
    if let Some(src) = resolve_source_from_disk(config) {
        refresh(config, &src);
    } else {
        log::warn!("overview-backdrop: no source on disk to refresh");
        if !config.niri_overview_backdrop() {
            kill_existing();
        }
    }
}

mod tests;
