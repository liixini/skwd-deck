#![cfg(test)]

use super::*;

#[test]
fn layer_shell_classes() {
    assert!(matches!(layer_shell_support("niri"), LayerShell::Yes));
    assert!(matches!(layer_shell_support("Hyprland"), LayerShell::Yes));
    assert!(matches!(layer_shell_support("sway"), LayerShell::Yes));
    assert!(matches!(layer_shell_support("KDE"), LayerShell::Yes));
    assert!(matches!(layer_shell_support("GNOME"), LayerShell::No));
    assert!(matches!(layer_shell_support("weird-wm"), LayerShell::Unknown));
}

#[test]
fn pickonly_trap_gate() {
    assert!(pickonly_trap(true, 0));
    assert!(!pickonly_trap(true, 2));
    assert!(!pickonly_trap(false, 0));
}

#[test]
fn solar_coords_warn() {
    assert!(solar_coords_unset(true, true, 0.0, 0.0));
    assert!(!solar_coords_unset(true, true, 51.5, -0.13));
    assert!(!solar_coords_unset(false, true, 0.0, 0.0));
    assert!(!solar_coords_unset(true, false, 0.0, 0.0));
    assert!(!solar_coords_unset(true, true, 0.0, -0.13));
}

#[test]
fn which_finds_sh() {
    assert!(which("sh").is_some());
    assert!(which("definitely-not-a-real-binary-xyz").is_none());
}

#[test]
fn renderer_capabilities_preserve_optional_and_misconfigured_states() {
    use skwd_wall_core::infrastructure::paper::{
        CapabilitiesResult, RendererCapability, RendererDiscovery, RuntimeDependencyStatus,
        SourceKind, VideoEngine,
    };

    let missing = RendererCapability {
        executable: "skwd-wall-vk".into(),
        source_kinds: vec![SourceKind::Video, SourceKind::WallpaperEngine],
        video_engines: vec![VideoEngine::Default],
        path: None,
        discovery: RendererDiscovery::Unresolved,
        present: false,
        executable_file: false,
        dependencies: vec![RuntimeDependencyStatus {
            name: "vulkan_loader".into(),
            available: false,
            detail: "libvulkan.so.1 is not loadable".into(),
        }],
        diagnostic: Some("skwd-wall-vk was not found".into()),
    };
    let configured = RendererCapability {
        executable: "skwd-wall-still".into(),
        source_kinds: vec![SourceKind::Static],
        video_engines: vec![],
        path: Some("/opt/skwd-wall-still".into()),
        discovery: RendererDiscovery::Configured,
        present: true,
        executable_file: false,
        dependencies: vec![],
        diagnostic: Some("skwd-wall-still is not executable".into()),
    };
    let blocked = RendererCapability {
        executable: "skwd-paper-tinier".into(),
        source_kinds: vec![SourceKind::Video],
        video_engines: vec![VideoEngine::Tinier],
        path: Some("/usr/lib/skwd-paper/skwd-paper-tinier".into()),
        discovery: RendererDiscovery::PrivateSibling,
        present: true,
        executable_file: true,
        dependencies: vec![RuntimeDependencyStatus {
            name: "wayland_connection".into(),
            available: false,
            detail: "Wayland compositor is not reachable".into(),
        }],
        diagnostic: None,
    };
    let capabilities =
        CapabilitiesResult::current().with_renderers(vec![missing, configured, blocked]);
    let mut report = Report::new();
    report_renderer_capabilities(&mut report, &capabilities);

    assert!(matches!(report.lines[0].0, Status::Warn));
    assert!(report.lines[0].2.contains("present=false; executable=false"));
    assert!(matches!(report.lines[1].0, Status::Warn));
    assert_eq!(report.lines[1].1, "skwd-wall-vk/vulkan_loader");
    assert!(matches!(report.lines[2].0, Status::Fail));
    assert!(report.lines[2].2.contains("source=configured"));
    assert!(report.lines[2].2.contains("path=/opt/skwd-wall-still"));
    assert!(matches!(report.lines[3].0, Status::Fail));
    assert!(matches!(report.lines[4].0, Status::Fail));
    assert_eq!(report.lines[4].1, "skwd-paper-tinier/wayland_connection");
}

#[test]
fn legacy_paper_capabilities_fail_closed() {
    let mut report = Report::new();
    report_renderer_capabilities(
        &mut report,
        &skwd_wall_core::infrastructure::paper::CapabilitiesResult::current(),
    );
    assert_eq!(report.lines.len(), 1);
    assert!(matches!(report.lines[0].0, Status::Fail));
    assert!(report.lines[0].2.contains("upgrade skwd-paper"));
}

#[test]
fn bug_report_redacts_keys() {
    let config = Config::from_root(serde_json::json!({
        "wallhaven": {"apiKey": "WALLHAVEN_SECRET"},
        "steam": {"apiKey": "STEAM_SECRET"},
        "sources": {
            "pexels": {"apiKey": "PEXELS_SECRET"},
            "unsplash": {"accessKey": "UNSPLASH_SECRET"}
        }
    }));
    let report = concat!(
        "url=https://wallhaven.cc/api?apikey=WALLHAVEN_SECRET\n",
        "provider echoed STEAM_SECRET PEXELS_SECRET UNSPLASH_SECRET\n",
        "ordinary diagnostic remains visible"
    );
    let safe = redact_bug_report(report, &config);

    for secret in ["WALLHAVEN_SECRET", "STEAM_SECRET", "PEXELS_SECRET", "UNSPLASH_SECRET"] {
        assert!(!safe.contains(secret));
    }
    assert!(safe.contains("apikey=[REDACTED]"));
    assert!(safe.contains("ordinary diagnostic remains visible"));
}
