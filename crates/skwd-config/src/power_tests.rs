use super::*;
use serde_json::json;
use std::sync::atomic::AtomicBool;

fn supply(
    root: &std::path::Path,
    name: &str,
    supply_type: Option<&str>,
    scope: Option<&str>,
    status: Option<&str>,
) {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    if let Some(supply_type) = supply_type {
        std::fs::write(dir.join("type"), supply_type).unwrap();
    }
    if let Some(scope) = scope {
        std::fs::write(dir.join("scope"), scope).unwrap();
    }
    if let Some(status) = status {
        std::fs::write(dir.join("status"), status).unwrap();
    }
}

#[test]
fn defaults_only_on_battery() {
    let root = json!({});
    assert!(battery_saver_enabled(&root));
    assert_eq!(effective_gpu_preference(&root, false), "none");
    assert_eq!(effective_gpu_preference(&root, true), "low");
    assert_eq!(effective_picker_fps(&root, false, 144.0), 144.0);
    assert_eq!(effective_picker_fps(&root, true, 144.0), 60.0);
    assert_eq!(effective_video_idle_seconds(&root, false, 0), 0);
    assert_eq!(effective_video_idle_seconds(&root, true, 0), 120);
    assert!(!effective_wallpaper_performance(&root, true, false));
}

#[test]
fn explicit_controls_override_caps() {
    let root = json!({"performance": {
        "batterySaver": true,
        "batteryFps": 0,
        "batteryVideoIdleSeconds": 30,
        "batteryWallpaperPerformance": true,
        "gpuPreference": "high"
    }});
    assert_eq!(effective_gpu_preference(&root, true), "high");
    assert_eq!(effective_picker_fps(&root, true, 165.0), 165.0);
    assert_eq!(effective_video_idle_seconds(&root, true, 90), 30);
    assert!(effective_wallpaper_performance(&root, true, false));

    let off = json!({"performance": {"batterySaver": false}});
    assert_eq!(effective_picker_fps(&off, true, 165.0), 165.0);
    assert_eq!(effective_video_idle_seconds(&off, true, 0), 0);
    assert!(!effective_wallpaper_performance(&off, true, false));
}

#[test]
fn device_batteries_ignored() {
    let root = tempfile::tempdir().unwrap();
    supply(root.path(), "hidpp_battery_0", Some("Battery"), Some("Device"), Some("Discharging"));
    assert_eq!(power_source_state_at(root.path()), PowerSourceState::NoSystemBattery);

    supply(root.path(), "BAT0", Some("Battery"), Some("System"), Some("Full"));
    assert_eq!(power_source_state_at(root.path()), PowerSourceState::ExternalPower);
}

#[test]
fn system_battery_scope_optional() {
    let root = tempfile::tempdir().unwrap();
    supply(root.path(), "BAT0", Some("Battery"), None, Some("Charging"));
    assert_eq!(power_source_state_at(root.path()), PowerSourceState::ExternalPower);

    supply(root.path(), "BAT1", Some("battery"), Some("system"), Some("DISCHARGING"));
    assert_eq!(power_source_state_at(root.path()), PowerSourceState::OnBattery);
}

#[test]
fn unreadable_state_unknown() {
    let missing = tempfile::tempdir().unwrap().path().join("gone");
    assert_eq!(power_source_state_at(&missing), PowerSourceState::Unknown);

    let root = tempfile::tempdir().unwrap();
    supply(root.path(), "BAT0", Some("Battery"), Some("System"), None);
    assert_eq!(power_source_state_at(root.path()), PowerSourceState::Unknown);

    std::fs::write(root.path().join("BAT0/status"), "Unknown").unwrap();
    assert_eq!(power_source_state_at(root.path()), PowerSourceState::Unknown);
}

#[test]
fn unreadable_supply_type_unknown() {
    let root = tempfile::tempdir().unwrap();
    supply(root.path(), "mystery", None, None, None);
    assert_eq!(power_source_state_at(root.path()), PowerSourceState::Unknown);
}

#[test]
fn unknown_keeps_last_known() {
    let last_known = AtomicBool::new(false);
    assert!(!on_battery_with_memory(PowerSourceState::Unknown, &last_known));
    assert!(on_battery_with_memory(PowerSourceState::OnBattery, &last_known));
    assert!(on_battery_with_memory(PowerSourceState::Unknown, &last_known));
    assert!(!on_battery_with_memory(PowerSourceState::ExternalPower, &last_known));
}

#[test]
fn percent_averages_system_batteries() {
    let root = tempfile::tempdir().unwrap();
    supply(root.path(), "BAT0", Some("Battery"), Some("System"), Some("Discharging"));
    supply(root.path(), "BAT1", Some("Battery"), None, Some("Discharging"));
    supply(root.path(), "mouse", Some("Battery"), Some("Device"), Some("Discharging"));
    std::fs::write(root.path().join("BAT0/capacity"), "20").unwrap();
    std::fs::write(root.path().join("BAT1/capacity"), "41").unwrap();
    std::fs::write(root.path().join("mouse/capacity"), "99").unwrap();
    assert_eq!(battery_percent_at(root.path()), Some(31));
}

#[test]
fn percent_none_without_capacity() {
    let root = tempfile::tempdir().unwrap();
    supply(root.path(), "BAT0", Some("Battery"), Some("System"), Some("Full"));
    assert_eq!(battery_percent_at(root.path()), None);
    std::fs::write(root.path().join("BAT0/capacity"), "101").unwrap();
    assert_eq!(battery_percent_at(root.path()), None);
}
