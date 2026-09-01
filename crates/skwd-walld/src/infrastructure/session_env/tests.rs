#![cfg(test)]

use std::collections::BTreeMap;

use super::*;

#[test]
fn manager_env_key_filter() {
    let environment = parse_manager_environment(
        "WAYLAND_DISPLAY=wayland-1\nXDG_CURRENT_DESKTOP=Hyprland\nEVIL=value\n",
    );
    assert_eq!(environment.get("WAYLAND_DISPLAY").map(String::as_str), Some("wayland-1"));
    assert_eq!(environment.get("XDG_CURRENT_DESKTOP").map(String::as_str), Some("Hyprland"));
    assert!(!environment.contains_key("EVIL"));
}

#[test]
fn overlay_replaces_stale() {
    let mut environment = BTreeMap::from([
        ("WAYLAND_DISPLAY".into(), "wayland-0".into()),
        ("XDG_CURRENT_DESKTOP".into(), "niri".into()),
    ]);
    overlay(
        &mut environment,
        BTreeMap::from([
            ("WAYLAND_DISPLAY".into(), "wayland-1".into()),
            ("XDG_CURRENT_DESKTOP".into(), "Hyprland".into()),
        ]),
    );
    assert_eq!(environment["WAYLAND_DISPLAY"], "wayland-1");
    assert_eq!(environment["XDG_CURRENT_DESKTOP"], "Hyprland");
}

#[test]
fn discovery_skips_non_sockets() {
    let runtime = tempfile::tempdir().unwrap();
    std::fs::write(runtime.path().join("wayland-8.lock"), b"").unwrap();
    std::fs::write(runtime.path().join("wayland-9"), b"").unwrap();
    std::fs::write(runtime.path().join("wayland-1"), b"").unwrap();
    std::fs::write(runtime.path().join("wayland-2"), b"").unwrap();
    let discovered = discover_wayland_socket_with(runtime.path(), |path| {
        matches!(path.file_name().and_then(|name| name.to_str()), Some("wayland-1" | "wayland-2"))
    });
    assert_eq!(discovered.as_deref(), Some("wayland-2"));
}

#[test]
fn display_path_relative_absolute() {
    let runtime = tempfile::tempdir().unwrap();
    let mut environment = BTreeMap::from([
        ("XDG_RUNTIME_DIR".into(), runtime.path().to_string_lossy().into_owned()),
        ("WAYLAND_DISPLAY".into(), "wayland-1".into()),
    ]);
    let socket = runtime.path().join("wayland-1");
    assert_eq!(wayland_display_path(&environment).as_deref(), Some(socket.as_path()));
    environment.insert("WAYLAND_DISPLAY".into(), socket.to_string_lossy().into_owned());
    assert_eq!(wayland_display_path(&environment).as_deref(), Some(socket.as_path()));
}
