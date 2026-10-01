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

pub fn restore_tinier(state: &crate::state::WallState) -> anyhow::Result<bool> {
    let config = state.config().clone();
    if config.renderer().video_engine() != "tinier" {
        return Ok(false);
    }
    let mut current = crate::audio::read_state(&config.cache_dir());
    let outputs = crate::outputs::enumerate();
    if let Some(map) = current.as_object_mut()
        && let Some(wildcard) = map.remove("*")
    {
        for output in &outputs {
            map.entry(output.name.clone()).or_insert_with(|| wildcard.clone());
        }
    }
    let adapter = super::PaperClientAdapter::configured(&config);
    let super::PaperCompositionPlan::Replace(request) =
        adapter.tinier_composition_plan(state, &current, &outputs)?
    else {
        return Ok(false);
    };
    if !request.assignments.iter().any(|entry| {
        entry.source.effective_video_engine() == Some(paper_control::VideoEngine::Tinier)
    }) {
        return Ok(false);
    }
    adapter.apply_request(request)?;
    crate::awww::stop();
    state.renderers().kill_all();
    let assignments = outputs
        .into_iter()
        .filter_map(|output| {
            let entry = current.get(&output.name).or_else(|| current.get("*"))?;
            Some((output.name, entry.get("path")?.as_str()?.to_string()))
        })
        .collect();
    state.renderers().replace_assignments(assignments);
    Ok(true)
}
