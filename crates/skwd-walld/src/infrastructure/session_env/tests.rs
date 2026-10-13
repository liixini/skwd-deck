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

#[test]
fn finds_niri_socket() {
    let runtime = tempfile::tempdir().unwrap();
    let bind =
        |name: &str| std::os::unix::net::UnixListener::bind(runtime.path().join(name)).unwrap();
    let _display = bind("wayland-1");
    let _other = bind(&format!("niri.wayland-2.{}.sock", std::process::id()));
    let _stale = bind("niri.wayland-1.4294967294.sock");
    let live = format!("niri.wayland-1.{}.sock", std::process::id());
    let _live = bind(&live);
    let mut environment = BTreeMap::from([
        ("XDG_RUNTIME_DIR".into(), runtime.path().to_string_lossy().into_owned()),
        ("WAYLAND_DISPLAY".into(), "wayland-1".into()),
    ]);
    adopt_compositor_ipc(&mut environment);
    assert_eq!(
        environment.get("NIRI_SOCKET").map(String::as_str),
        Some(runtime.path().join(&live).to_string_lossy().as_ref())
    );
    environment.insert("NIRI_SOCKET".into(), "/run/user/1000/niri.imported.sock".into());
    adopt_compositor_ipc(&mut environment);
    assert_eq!(environment["NIRI_SOCKET"], runtime.path().join(&live).to_string_lossy());
}

#[test]
fn niri_waits_for_ipc() {
    let runtime = tempfile::tempdir().unwrap();
    let _display =
        std::os::unix::net::UnixListener::bind(runtime.path().join("wayland-1")).unwrap();
    let other = format!("niri.wayland-2.{}.sock", std::process::id());
    let _other = std::os::unix::net::UnixListener::bind(runtime.path().join(other)).unwrap();
    let mut environment = BTreeMap::from([
        ("XDG_RUNTIME_DIR".into(), runtime.path().to_string_lossy().into_owned()),
        ("WAYLAND_DISPLAY".into(), "wayland-1".into()),
        ("XDG_CURRENT_DESKTOP".into(), "niri:GNOME".into()),
    ]);
    assert!(ready_environment(environment.clone()).is_none());
    let live = format!("niri.wayland-1.{}.sock", std::process::id());
    let socket = runtime.path().join(live);
    let orphan = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    drop(orphan);
    environment.insert(NIRI_SOCKET_KEY.into(), socket.to_string_lossy().into_owned());
    assert!(ready_environment(environment.clone()).is_none());
    std::fs::remove_file(&socket).unwrap();
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let ready = ready_environment(environment.clone()).unwrap();
    assert_eq!(ready["NIRI_SOCKET"], socket.to_string_lossy());
    environment.insert(
        "NIRI_SOCKET".into(),
        runtime.path().join("missing.sock").to_string_lossy().into_owned(),
    );
    assert_eq!(ready_environment(environment).unwrap()["NIRI_SOCKET"], socket.to_string_lossy());
    drop(listener);
}

#[test]
fn niri_backlog_recovers() {
    let runtime = tempfile::tempdir().unwrap();
    let _display =
        std::os::unix::net::UnixListener::bind(runtime.path().join("wayland-1")).unwrap();
    let socket = runtime.path().join(format!("niri.wayland-1.{}.sock", std::process::id()));
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    assert_eq!(unsafe { libc::listen(listener.as_raw_fd(), 0) }, 0);
    let _queued = std::os::unix::net::UnixStream::connect(&socket).unwrap();
    let environment = BTreeMap::from([
        ("XDG_RUNTIME_DIR".into(), runtime.path().to_string_lossy().into_owned()),
        ("WAYLAND_DISPLAY".into(), "wayland-1".into()),
        ("XDG_CURRENT_DESKTOP".into(), "niri".into()),
    ]);

    let started = Instant::now();
    assert!(ready_environment(environment.clone()).is_none());
    assert!(started.elapsed() < Duration::from_millis(500));

    let _accepted = listener.accept().unwrap();
    assert_eq!(ready_environment(environment).unwrap()[NIRI_SOCKET_KEY], socket.to_string_lossy());
}

#[test]
fn other_desktop_ready() {
    let runtime = tempfile::tempdir().unwrap();
    let environment = BTreeMap::from([
        ("XDG_RUNTIME_DIR".into(), runtime.path().to_string_lossy().into_owned()),
        ("WAYLAND_DISPLAY".into(), "wayland-1".into()),
        ("XDG_CURRENT_DESKTOP".into(), "KDE".into()),
    ]);
    assert!(ready_environment(environment.clone()).is_none());
    let _display =
        std::os::unix::net::UnixListener::bind(runtime.path().join("wayland-1")).unwrap();
    assert!(ready_environment(environment).is_some());
}

#[test]
fn skips_invalid_niri_sockets() {
    let runtime = tempfile::tempdir().unwrap();
    let _display =
        std::os::unix::net::UnixListener::bind(runtime.path().join("wayland-1")).unwrap();
    std::fs::write(runtime.path().join(format!("niri.wayland-1.{}.sock", std::process::id())), b"")
        .unwrap();
    let _stale = std::os::unix::net::UnixListener::bind(
        runtime.path().join("niri.wayland-1.4294967294.sock"),
    )
    .unwrap();
    let mut environment = BTreeMap::from([
        ("XDG_RUNTIME_DIR".into(), runtime.path().to_string_lossy().into_owned()),
        ("WAYLAND_DISPLAY".into(), "wayland-1".into()),
    ]);
    adopt_compositor_ipc(&mut environment);
    assert!(!environment.contains_key("NIRI_SOCKET"));
}

#[test]
fn finds_hyprland_signature() {
    let runtime = tempfile::tempdir().unwrap();
    let _display =
        std::os::unix::net::UnixListener::bind(runtime.path().join("wayland-1")).unwrap();
    let instance = |signature: &str, lock: String| {
        let dir = runtime.path().join("hypr").join(signature);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("hyprland.lock"), lock).unwrap();
    };
    let pid = std::process::id();
    instance("stale_1_1", "4294967294\nwayland-1\n".into());
    instance("nested_1_1", format!("{pid}\nwayland-2\n"));
    instance("live_1_1", format!("{pid}\nwayland-1\n"));
    let mut environment = BTreeMap::from([
        ("XDG_RUNTIME_DIR".into(), runtime.path().to_string_lossy().into_owned()),
        ("WAYLAND_DISPLAY".into(), "wayland-1".into()),
    ]);
    adopt_compositor_ipc(&mut environment);
    assert_eq!(
        environment.get("HYPRLAND_INSTANCE_SIGNATURE").map(String::as_str),
        Some("live_1_1")
    );
    assert!(!environment.contains_key("NIRI_SOCKET"));
    environment.insert("HYPRLAND_INSTANCE_SIGNATURE".into(), "imported".into());
    adopt_compositor_ipc(&mut environment);
    assert_eq!(environment["HYPRLAND_INSTANCE_SIGNATURE"], "imported");
}

#[test]
fn skips_invalid_hyprland_instances() {
    let runtime = tempfile::tempdir().unwrap();
    let _display =
        std::os::unix::net::UnixListener::bind(runtime.path().join("wayland-1")).unwrap();
    for (signature, lock) in [
        ("stale_1_1", "4294967294\nwayland-1\n".to_owned()),
        ("nested_1_1", format!("{}\nwayland-2\n", std::process::id())),
        ("garbled_1_1", "not-a-pid\nwayland-1\n".to_owned()),
    ] {
        let dir = runtime.path().join("hypr").join(signature);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("hyprland.lock"), lock).unwrap();
    }
    let mut environment = BTreeMap::from([
        ("XDG_RUNTIME_DIR".into(), runtime.path().to_string_lossy().into_owned()),
        ("WAYLAND_DISPLAY".into(), "wayland-1".into()),
    ]);
    adopt_compositor_ipc(&mut environment);
    assert!(!environment.contains_key("HYPRLAND_INSTANCE_SIGNATURE"));
}
