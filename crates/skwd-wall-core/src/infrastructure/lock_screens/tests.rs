use super::*;
use serde_json::json;

#[test]
fn defaults_stay_off() {
    assert!(!enabled(&json!({})));
    assert!(!follows(&json!({"lockScreens":{"mode":"invalid"}})));
    assert!(follows(&json!({"lockScreens":{"mode":"follow"}})));
}

#[test]
fn swaylock_restores_edits() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config");
    let record = temp.path().join("receipt");
    std::fs::write(&config, "clock\nimage=old.png\n").unwrap();
    ownership::manage(&config, &record, true, |_| {
        Ok(vec![ownership::Patch {
            before: "image=old.png\n".into(),
            after: "image=new.png\n".into(),
            retained_section: None,
        }])
    })
    .unwrap();
    std::fs::write(&config, "clock\nimage=new.png\nfont=Sans\n").unwrap();
    ownership::manage(&config, &record, false, |_| unreachable!()).unwrap();
    assert_eq!(std::fs::read_to_string(config).unwrap(), "clock\nimage=old.png\nfont=Sans\n");
}

#[test]
fn changed_ownership_is_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config");
    let record = temp.path().join("receipt");
    ownership::manage(&config, &record, true, |_| {
        Ok(vec![ownership::Patch {
            before: String::new(),
            after: "image=new.png\n".into(),
            retained_section: None,
        }])
    })
    .unwrap();
    std::fs::write(&config, "image=user.png\n").unwrap();
    assert!(ownership::manage(&config, &record, false, |_| unreachable!()).is_err());
    assert_eq!(std::fs::read_to_string(config).unwrap(), "image=user.png\n");
    assert!(record.exists());
}

#[test]
fn symlink_configs_are_refused() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config");
    let original = temp.path().join("original");
    std::fs::write(&original, "unchanged").unwrap();
    std::os::unix::fs::symlink(&original, &config).unwrap();
    assert!(ownership::write(&config, "changed").is_err());
    assert_eq!(std::fs::read_to_string(original).unwrap(), "unchanged");
}

#[test]
fn hyprlock_preserves_layout() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("state");
    let config_home = temp.path().join("config");
    std::fs::create_dir_all(config_home.join("hypr")).unwrap();
    let path = config_home.join("hypr/hyprlock.conf");
    let original = "background {\n    monitor = DP-1\n    path = original.png\n    blur_passes = 3\n}\nlabel {\n    text = Password\n}\n";
    std::fs::write(&path, original).unwrap();
    let config = json!({"lockScreens":{"mode":"follow","hyprlock":true}});
    text::sync(&config, Some(&root.join(IMAGE_FILE)), &root, &config_home).unwrap();
    let installed = std::fs::read_to_string(&path).unwrap();
    assert!(installed.starts_with("source = "));
    assert!(installed.contains("    path = $skwd_lock_image_0\n    blur_passes = 3"));
    assert!(installed.contains("label {\n    text = Password\n}"));
    std::fs::write(&path, installed.replace("blur_passes = 3", "blur_passes = 5")).unwrap();
    text::sync(&json!({}), None, &root, &config_home).unwrap();
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        original.replace("blur_passes = 3", "blur_passes = 5")
    );
}

#[test]
fn hyprlock_fills_missing_widgets() {
    let cases = [
        "",
        "input-field {\n    size = 240, 40\n}\n",
        "background {\n    blur_passes = 2\n}\nlabel {\n    text = User\n}\n",
    ];
    for original in cases {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("state");
        let home = temp.path().join("config");
        let path = home.join("hypr/hyprlock.conf");
        if !original.is_empty() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, original).unwrap();
        }
        let config = json!({"lockScreens":{"mode":"follow","hyprlock":true}});
        text::sync_hyprlock(&config, Some(&root.join(IMAGE_FILE)), &root, &home).unwrap();
        let installed = std::fs::read_to_string(&path).unwrap();
        assert!(
            installed.contains("background {") && installed.contains("path = $skwd_lock_image_0")
        );
        assert_eq!(installed.matches("input-field {").count(), 1);
        text::sync_hyprlock(&json!({}), None, &root, &home).unwrap();
        if original.is_empty() {
            assert!(!path.exists());
        } else {
            assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        }
    }
}

#[test]
fn targets_fail_independently() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("state");
    let config_home = temp.path().join("config");
    let hypr = config_home.join("hypr/hyprlock.conf");
    std::fs::create_dir_all(hypr.parent().unwrap()).unwrap();
    std::fs::write(&hypr, "background {\n    path = first.png\n    path = second.png\n}\n")
        .unwrap();
    let config = json!({"lockScreens":{"mode":"follow","hyprlock":true,"swaylock":true}});
    assert!(text::sync(&config, Some(&root.join(IMAGE_FILE)), &root, &config_home).is_err());
    assert!(
        std::fs::read_to_string(config_home.join("swaylock/config")).unwrap().starts_with("image=")
    );
}

#[test]
fn pending_install_recovers() {
    for enabled in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("config");
        let record = temp.path().join("receipt");
        std::fs::write(&config, "image=old.png\nfont=Sans\n").unwrap();
        std::fs::write(
            &record,
            json!({
                "version":1,"config":config,"pending":true,"created":false,
                "patches":[{"before":"image=old.png\n","after":"image=new.png\n"}]
            })
            .to_string(),
        )
        .unwrap();
        ownership::manage(&config, &record, enabled, |_| unreachable!()).unwrap();
        let expected =
            if enabled { "image=new.png\nfont=Sans\n" } else { "image=old.png\nfont=Sans\n" };
        assert_eq!(std::fs::read_to_string(&config).unwrap(), expected);
        if enabled {
            ownership::manage(&config, &record, false, |_| unreachable!()).unwrap();
        }
        assert!(!record.exists());
        assert_eq!(std::fs::read_to_string(config).unwrap(), "image=old.png\nfont=Sans\n");
    }
}

#[test]
fn missing_source_keeps_ownership() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("state");
    let home = temp.path().join("config");
    let config = json!({"lockScreens":{"mode":"follow","swaylock":true}});
    text::sync(&config, Some(&root.join(IMAGE_FILE)), &root, &home).unwrap();
    let path = home.join("swaylock/config");
    let before = std::fs::read(&path).unwrap();
    text::sync(&config, None, &root, &home).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(root.join("swaylock.json").exists());
    text::sync(&json!({}), None, &root, &home).unwrap();
    assert!(!path.exists());
}

#[test]
fn legacy_sway_stays_owned() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let config_home = home.join(".config");
    let legacy = home.join(".swaylock/config");
    let xdg = config_home.join("swaylock/config");
    let record = temp.path().join("swaylock.json");
    std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    std::fs::write(&legacy, "image=old.png\n").unwrap();
    let selected = text::sway_path(&config_home, &home);
    assert_eq!(selected, legacy);
    ownership::manage(&selected, &record, true, |original| {
        text::swaylock(original, &temp.path().join("wallpaper.png"))
    })
    .unwrap();
    std::fs::remove_file(&legacy).unwrap();
    let selected = text::sway_path(&config_home, &home);
    assert_eq!(selected, xdg);
    assert!(ownership::manage(&selected, &record, true, |_| unreachable!()).is_err());
    assert!(!xdg.exists());
    assert!(record.exists());
}

#[test]
fn new_legacy_blocks_switch() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let config_home = home.join(".config");
    let xdg = config_home.join("swaylock/config");
    let legacy = home.join(".swaylock/config");
    let record = temp.path().join("swaylock.json");
    std::fs::create_dir_all(xdg.parent().unwrap()).unwrap();
    std::fs::write(&xdg, "image=old.png\n").unwrap();
    ownership::manage(&text::sway_path(&config_home, &home), &record, true, |original| {
        text::swaylock(original, &temp.path().join("wallpaper.png"))
    })
    .unwrap();
    std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    std::fs::write(&legacy, "image=user.png\n").unwrap();
    assert!(
        ownership::manage(
            &text::sway_path(&config_home, &home),
            &record,
            false,
            |_| unreachable!()
        )
        .is_err()
    );
    assert_eq!(std::fs::read_to_string(&legacy).unwrap(), "image=user.png\n");
    assert!(record.exists());
}

#[test]
fn readonly_config_is_preserved() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config");
    std::fs::write(&path, "original").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
    assert!(ownership::write(&path, "changed").is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "original");
}
