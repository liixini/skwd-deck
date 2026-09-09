use std::path::PathBuf;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

use anyhow::{Result, bail};
use skwd_wall_core::config::Config;
use skwd_wall_core::infrastructure::paper::{
    ApplyRequest, Assignment, Layer, PaperClient, Source, SourceKind, SurfacePolicy,
    renderer_policy,
};

const NAMESPACE: &str = "skwd-paper-backdrop";
static OPEN: AtomicBool = AtomicBool::new(false);
static BACKDROP: Mutex<Backdrop> = Mutex::new(Backdrop { client: None, paused: None });

struct Backdrop {
    client: Option<PaperClient>,
    paused: Option<bool>,
}

fn is_niri() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|desk| desk.to_lowercase().contains("niri"))
}

fn parse_backdrop_source(json_text: &str) -> Option<String> {
    let val: serde_json::Value = serde_json::from_str(json_text).ok()?;
    if let Some(id) = val
        .get("we_id")
        .and_then(serde_json::Value::as_str)
        .filter(|id| skwd_wall_core::we::valid_we_id(id))
    {
        return Some(format!("we:{id}"));
    }
    val.get("path")
        .and_then(serde_json::Value::as_str)
        .filter(|path| !path.is_empty())
        .map(String::from)
}

fn pick_backdrop_source(follow: bool, fixed: &str, last_json: &str) -> Option<String> {
    if !follow && !fixed.trim().is_empty() {
        return Some(fixed.trim().to_string());
    }
    parse_backdrop_source(last_json)
}

pub fn resolve_source_from_disk(config: &Config) -> Option<String> {
    let last =
        std::fs::read_to_string(skwd_wall_core::paths::cache_dir().join("last-wallpaper.json"))
            .unwrap_or_default();
    pick_backdrop_source(
        config.niri_backdrop_follow_wallpaper(),
        &config.niri_backdrop_source(),
        &last,
    )
}

fn media_source(config: &Config, source: &str) -> Result<Source> {
    let path = if let Some(id) = source.strip_prefix("we:") {
        if !skwd_wall_core::we::valid_we_id(id) {
            bail!("invalid Wallpaper Engine backdrop id");
        }
        config.we_dir().join(id)
    } else if let Some(relative) = source.strip_prefix("~/") {
        PathBuf::from(
            std::env::var_os("HOME").ok_or_else(|| anyhow::anyhow!("HOME is unavailable"))?,
        )
        .join(relative)
    } else {
        PathBuf::from(source)
    };
    if !path.exists() {
        bail!("backdrop source does not exist: {}", path.display());
    }
    let text = path.to_string_lossy().into_owned();
    if path.is_dir() {
        Ok(Source::wallpaper_engine(text))
    } else if skwd_wall_core::paths::is_video_path(&path) {
        Ok(Source::video(text, None))
    } else {
        Ok(Source::static_file(text))
    }
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

fn client(config: &Config) -> PaperClient {
    let socket = skwd_wall_core::infrastructure::paper::paper_socket_path()
        .with_file_name("overview-backdrop.sock");
    PaperClient::new(config.renderer().paper_bin(), socket)
}

fn pause(backdrop: &mut Backdrop) -> Result<()> {
    let paused = !OPEN.load(Ordering::Acquire);
    if backdrop.paused != Some(paused)
        && let Some(client) = &backdrop.client
    {
        client.set_paused(paused)?;
        backdrop.paused = Some(paused);
    }
    Ok(())
}

pub fn set_overview(open: Option<bool>) {
    OPEN.store(open == Some(true), Ordering::Release);
    let mut backdrop = BACKDROP.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Err(error) = pause(&mut backdrop) {
        log::warn!("overview-backdrop: {error}");
    }
}

fn legacy_session_matches(environment: &[u8], display: &str, runtime: &str) -> bool {
    [format!("WAYLAND_DISPLAY={display}"), format!("XDG_RUNTIME_DIR={runtime}")].iter().all(
        |expected| environment.split(|byte| *byte == 0).any(|entry| entry == expected.as_bytes()),
    )
}

fn stop_legacy() {
    let (Ok(display), Ok(runtime)) =
        (std::env::var("WAYLAND_DISPLAY"), std::env::var("XDG_RUNTIME_DIR"))
    else {
        return;
    };
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<i32>() else {
            continue;
        };
        let Ok(command) = std::fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        let args: Vec<_> = command.split(|byte| *byte == 0).collect();
        if args.first().is_some_and(|arg| arg.ends_with(b"/skwd-wall-still"))
            && !args.contains(&b"--persist".as_slice())
            && args.windows(2).any(|pair| pair == [b"--namespace".as_slice(), NAMESPACE.as_bytes()])
        {
            let Ok(environment) = std::fs::read(entry.path().join("environ")) else {
                continue;
            };
            if legacy_session_matches(&environment, &display, &runtime) {
                unsafe {
                    libc::kill(pid, libc::SIGTERM);
                }
            }
        }
    }
}

pub fn refresh_from_disk(config: &Config) -> Result<()> {
    let mut backdrop = BACKDROP.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let client = client(config);
    if !is_niri() || !config.niri_overview_backdrop() {
        stop_legacy();
        let socket = skwd_wall_core::infrastructure::paper::paper_socket_path()
            .with_file_name("overview-backdrop.sock");
        if socket.exists() {
            client.stop(Vec::new())?;
        }
        backdrop.client = None;
        backdrop.paused = None;
        return Ok(());
    }
    let source = resolve_source_from_disk(config)
        .ok_or_else(|| anyhow::anyhow!("no overview backdrop selected"))?;
    let mut source = media_source(config, &source)?;
    if source.kind == SourceKind::Static {
        source.path = themed_source(config, &source.path);
    }
    let mut policy = renderer_policy(config, &[]);
    policy.idle_seconds = Some(0);
    policy.transitions_enabled = Some(false);
    policy.surface = Some(Box::new(SurfacePolicy {
        namespace: NAMESPACE.into(),
        blur: if config.niri_backdrop_blur_enabled() {
            config.niri_backdrop_blur().clamp(0.0, 100.0) as u32
        } else {
            0
        },
        dim: config.niri_backdrop_dim().min(100),
    }));
    let mut assignment = Assignment::new(vec!["*".into()], source);
    assignment.layer = Layer::Background;
    if !client.capabilities()?.renderer_policy.surface {
        bail!("Paper does not support overview surfaces; update the Paper renderer");
    }
    client.set_paused(false)?;
    let applied = client.apply(ApplyRequest {
        assignments: vec![assignment],
        replace_all: true,
        policy: Some(policy),
    });
    backdrop.client = Some(client);
    backdrop.paused = Some(false);
    let paused = pause(&mut backdrop);
    applied?;
    paused?;
    stop_legacy();
    log::info!("overview-backdrop: ready");
    Ok(())
}

pub fn settings(config: &Config) -> serde_json::Value {
    serde_json::json!([
        config.niri_overview_backdrop(),
        config.niri_backdrop_source(),
        config.niri_backdrop_follow_wallpaper(),
        config.niri_backdrop_blur_enabled(),
        config.niri_backdrop_blur(),
        config.niri_backdrop_dim(),
        config.niri_backdrop_auto_theme(),
        config.niri_backdrop_theme()
    ])
}

pub fn on_apply(config: &Config) {
    if config.niri_backdrop_follow_wallpaper()
        && let Err(error) = refresh_from_disk(config)
    {
        log::warn!("overview-backdrop: {error}");
    }
}

mod tests;
