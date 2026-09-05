use super::{
    PLUGIN_ID, assignments, desktop_is_plasma, enabled_for, plugin_installed_in, qdbus_program_in,
    script,
};

#[test]
fn desktop_tokens() {
    assert!(desktop_is_plasma("KDE"));
    assert!(desktop_is_plasma("GNOME:Plasma"));
    assert!(!desktop_is_plasma("niri"));
    assert!(!desktop_is_plasma("ukdesktop"));
}

#[test]
fn plugin_data_roots() {
    let directory = tempfile::tempdir().unwrap();
    assert!(!plugin_installed_in(&[directory.path().to_path_buf()]));
    let metadata = directory.path().join("plasma/wallpapers").join(PLUGIN_ID).join("metadata.json");
    std::fs::create_dir_all(metadata.parent().unwrap()).unwrap();
    std::fs::write(metadata, "{}").unwrap();
    assert!(plugin_installed_in(&[directory.path().to_path_buf()]));
    assert!(enabled_for("KDE", &[directory.path().to_path_buf()], false));
    assert!(!enabled_for("niri", &[directory.path().to_path_buf()], false));
    assert!(!enabled_for("KDE", &[directory.path().to_path_buf()], true));
}

#[test]
fn qdbus_prefers_arch_name_and_accepts_fedora_name() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let fedora = directory.path().join("qdbus-qt6");
    std::fs::write(&fedora, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&fedora, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(qdbus_program_in(Some(directory.path().as_os_str())), Some(fedora.clone()));

    let arch = directory.path().join("qdbus6");
    std::fs::write(&arch, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&arch, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(qdbus_program_in(Some(directory.path().as_os_str())), Some(arch));
    assert_eq!(qdbus_program_in(None), None);
}

#[test]
fn assignments_are_connector_keyed_and_complete() {
    let state = crate::state::WallState::test_new(serde_json::json!({
        "display": {"fillModes": {"DP-2": "fit"}}
    }));
    state.renderers().set_paused(true);
    state
        .with_db(|connection| {
            crate::db::set_we_property(connection, "123", "speed", Some(&serde_json::json!(2)))
        })
        .unwrap();
    let outputs = vec![crate::outputs::OutputInfo {
        name: "DP-2".to_string(),
        width: 2560,
        height: 1440,
        refresh_mhz: 144_000,
        ..Default::default()
    }];
    let map = serde_json::json!({
        "DP-2": {
            "type": "we",
            "path": "/scene/123",
            "we_id": "123",
            "mute": false,
            "volume": 37
        }
    });
    let payload =
        assignments(&state, &outputs, map.as_object().unwrap(), &std::collections::BTreeMap::new())
            .unwrap();
    let wrapper = &payload["DP-2"];
    let entry = &wrapper["assignment"];
    assert_eq!(entry["source"]["path"], "/scene/123");
    assert_eq!(entry["fill_mode"], "fit");
    assert_eq!(wrapper["fps"], 30);
    assert_eq!(entry["mute"], false);
    assert_eq!(entry["volume"], 37);
    assert_eq!(wrapper["paused"], true);
    assert_eq!(entry["source"]["properties"]["speed"], 2);
    assert!(wrapper["paper"].as_str().is_some());
    assert!(payload.get("0").is_none());
}

#[test]
fn assignment_carries_only_the_requested_transition() {
    let state = crate::state::WallState::test_new(serde_json::json!({}));
    let outputs = ["DP-1", "DP-2"].map(|name| crate::outputs::OutputInfo {
        name: name.to_string(),
        width: 1920,
        height: 1080,
        ..Default::default()
    });
    let map = serde_json::json!({
        "DP-1": {"type": "static", "path": "/wall/a.png"},
        "DP-2": {"type": "static", "path": "/wall/b.png"}
    });
    let transitions = std::collections::BTreeMap::from([(
        "DP-2".to_string(),
        crate::infrastructure::paper::TransitionPolicy {
            from: Some("/wall/old.png".into()),
            effect: Some("inkwell-drop".into()),
            duration_ms: Some(700),
        },
    )]);
    let payload = assignments(&state, &outputs, map.as_object().unwrap(), &transitions).unwrap();
    assert!(payload["DP-1"]["assignment"].get("transition").is_none());
    assert_eq!(payload["DP-2"]["assignment"]["transition"]["from"], "/wall/old.png");
    assert_eq!(payload["DP-2"]["assignment"]["transition"]["effect"], "inkwell-drop");
    assert_eq!(payload["DP-2"]["assignment"]["transition"]["duration_ms"], 700);
}

#[test]
fn plasma_script_publishes_one_connector_map() {
    let payload = serde_json::json!({"DP-1": {"kind": "video"}});
    let source = script(&payload);
    assert!(source.contains("JSON.stringify(a)"));
    assert!(source.contains("writeConfig(\"Assignments\",encoded)"));
    assert!(source.contains("\"DP-1\""));
    assert!(!source.contains("d.screen"));
}

#[test]
fn lock_screen_uses_the_same_paper_connector() {
    let source = include_str!("../plasma.rs");
    assert!(source.contains("use_lock_screen_paper"));
    assert!(source.contains("kconfig_write(&groups, \"Assignment\""));
    assert!(source.contains("kconfig_write(&groups, \"Paper\""));
    assert!(!source.contains("org.kde.image"));
    assert!(!source.contains("use_lock_screen_image"));
}

#[test]
fn plasma_without_its_plugin_reports_the_missing_package() {
    let root = tempfile::tempdir().unwrap();
    let roots = [root.path().to_path_buf()];
    assert!(
        super::require_backend_for("KDE", &roots, false)
            .unwrap_err()
            .to_string()
            .contains("skwd-paper-plasma")
    );
    assert!(super::require_backend_for("niri", &roots, false).is_ok());
    assert!(super::require_backend_for("KDE", &roots, true).is_ok());
    let metadata = root.path().join("plasma/wallpapers").join(PLUGIN_ID).join("metadata.json");
    std::fs::create_dir_all(metadata.parent().unwrap()).unwrap();
    std::fs::write(metadata, "{}").unwrap();
    assert!(super::require_backend_for("KDE", &roots, false).is_ok());
}
