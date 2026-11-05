use std::sync::atomic::{AtomicBool, Ordering};

use crate::config::Config;
use paper_control::SurfacePolicy;

static OVERVIEW: AtomicBool = AtomicBool::new(false);

pub fn set_stationary_overview(open: bool) {
    OVERVIEW.store(open, Ordering::Release);
}

pub fn stationary_surface(config: &Config, kind: paper_control::SourceKind) -> SurfacePolicy {
    let open = super::stationary_wallpaper(config) && OVERVIEW.load(Ordering::Acquire);
    SurfacePolicy {
        namespace: "skwd-paper-stationary".into(),
        blur: if open { config.niri_backdrop_blur(kind) } else { 0 },
        dim: if open { config.niri_backdrop_dim() } else { 0 },
    }
}

pub fn stationary_surfaces(config: &Config) -> paper_control::SurfaceOverridesRequest {
    paper_control::SurfaceOverridesRequest {
        surfaces: [
            paper_control::SourceKind::Static,
            paper_control::SourceKind::Video,
            paper_control::SourceKind::WallpaperEngine,
        ]
        .into_iter()
        .map(|kind| (kind, stationary_surface(config, kind)))
        .collect(),
    }
}

pub(crate) fn configure_stationary(
    command: &mut std::process::Command,
    config: &Config,
    kind: paper_control::SourceKind,
) {
    let still = kind == paper_control::SourceKind::Static;
    for key in
        ["SKWD_PAPER_NAMESPACE", "SKWD_PAPER_DYNAMIC_EFFECTS", "SKWD_PAPER_BLUR", "SKWD_PAPER_DIM"]
    {
        command.env_remove(key);
    }
    if !super::stationary_wallpaper(config) {
        return;
    }
    let surface = stationary_surface(config, kind);
    command
        .env("SKWD_PAPER_NAMESPACE", &surface.namespace)
        .env("SKWD_PAPER_DYNAMIC_EFFECTS", "1")
        .env("SKWD_PAPER_BLUR", surface.blur.to_string())
        .env("SKWD_PAPER_DIM", surface.dim.to_string())
        .env("SKWD_VK_LAYER", "background");
    if still {
        command
            .arg("--namespace")
            .arg(surface.namespace)
            .arg("--blur")
            .arg(surface.blur.to_string())
            .arg("--dim")
            .arg(surface.dim.to_string());
    }
}
