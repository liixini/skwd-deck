use super::*;

use serde_json::json;

fn setup(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn dms_config() -> Value {
    json!({"lockScreens": {"mode": "follow", "dms": true}})
}

fn noctalia_config() -> Value {
    json!({"lockScreens": {"mode": "follow", "noctalia": true}})
}

fn off_config() -> Value {
    json!({"lockScreens": {"mode": "off"}})
}

#[test]
fn dms_restores_field() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config/settings.json");
    let root = dir.path().join("receipts");
    let first = dir.path().join("first.png");
    let second = dir.path().join("second.png");
    setup(&path, r#"{"lockScreenWallpaperPath":"/old.png","matugenScheme":"original"}"#);
    dms_at(&dms_config(), Some(&first), &root, &path).unwrap();
    let applied: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(applied[DMS_FIELD], first.to_str().unwrap());
    assert_eq!(applied["matugenScheme"], "original");
    dms_at(&dms_config(), Some(&second), &root, &path).unwrap();
    dms_at(&off_config(), None, &root, &path).unwrap();
    let restored: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(restored[DMS_FIELD], "/old.png");
    assert_eq!(restored["matugenScheme"], "original");
    assert!(!root.join(DMS_RECEIPT).exists());
}

#[test]
fn dms_respects_edits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config/settings.json");
    let root = dir.path().join("receipts");
    let image = dir.path().join("first.png");
    setup(&path, r#"{"theme":"keep"}"#);
    dms_at(&dms_config(), Some(&image), &root, &path).unwrap();
    setup(&path, r#"{"lockScreenWallpaperPath":"/user.png","theme":"keep"}"#);
    assert!(dms_at(&dms_config(), Some(&image), &root, &path).is_err());
    dms_at(&off_config(), None, &root, &path).unwrap();
    let result: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(result[DMS_FIELD], "/user.png");
    assert!(!root.join(DMS_RECEIPT).exists());
}

#[test]
fn dms_rejects_bad_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config/settings.json");
    let root = dir.path().join("receipts");
    let image = dir.path().join("first.png");
    setup(&path, "not-json");
    assert!(dms_at(&dms_config(), Some(&image), &root, &path).is_err());
    assert!(!root.join(DMS_RECEIPT).exists());
    setup(&path, r#"{"lockScreenWallpaperPath":false}"#);
    assert!(dms_at(&dms_config(), Some(&image), &root, &path).is_err());
    assert!(!root.join(DMS_RECEIPT).exists());
}

#[test]
fn shells_create_settings() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("receipts");
    let image = dir.path().join("first.png");
    let dms = dir.path().join("dms.json");
    let noctalia = dir.path().join("settings.toml");
    dms_at(&dms_config(), Some(&image), &root, &dms).unwrap();
    noctalia_at(&noctalia_config(), Some(&image), &root, &noctalia).unwrap();
    let settings: Value = serde_json::from_str(&std::fs::read_to_string(&dms).unwrap()).unwrap();
    assert_eq!(settings[DMS_FIELD], image.to_str().unwrap());
    let settings: DocumentMut = std::fs::read_to_string(&noctalia).unwrap().parse().unwrap();
    assert_eq!(settings[LOCKSCREEN][WALLPAPER].as_str(), image.to_str());
    dms_at(&off_config(), None, &root, &dms).unwrap();
    noctalia_at(&off_config(), None, &root, &noctalia).unwrap();
    assert!(!dms.exists());
    assert!(!noctalia.exists());
}

#[test]
fn shells_keep_missing_image() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("receipts");
    let image = dir.path().join("first.png");
    let dms_path = dir.path().join("config/settings.json");
    let noctalia_path = dir.path().join("state/settings.toml");
    setup(&dms_path, r#"{"lockScreenWallpaperPath":"/old.png"}"#);
    setup(&noctalia_path, "config_version = 1\n[lockscreen]\nwallpaper = '/old.png'\n");
    dms_at(&dms_config(), Some(&image), &root, &dms_path).unwrap();
    noctalia_at(&noctalia_config(), Some(&image), &root, &noctalia_path).unwrap();
    let dms_live = std::fs::read_to_string(&dms_path).unwrap();
    let noctalia_live = std::fs::read_to_string(&noctalia_path).unwrap();
    dms_at(&dms_config(), None, &root, &dms_path).unwrap();
    noctalia_at(&noctalia_config(), None, &root, &noctalia_path).unwrap();
    assert_eq!(std::fs::read_to_string(&dms_path).unwrap(), dms_live);
    assert_eq!(std::fs::read_to_string(&noctalia_path).unwrap(), noctalia_live);
    assert!(root.join(DMS_RECEIPT).exists());
    assert!(root.join(NOCTALIA_RECEIPT).exists());
    dms_at(&off_config(), None, &root, &dms_path).unwrap();
    noctalia_at(&off_config(), None, &root, &noctalia_path).unwrap();
    let dms_restored: Value =
        serde_json::from_str(&std::fs::read_to_string(dms_path).unwrap()).unwrap();
    let noctalia_restored: DocumentMut =
        std::fs::read_to_string(noctalia_path).unwrap().parse().unwrap();
    assert_eq!(dms_restored[DMS_FIELD], "/old.png");
    assert_eq!(noctalia_restored[LOCKSCREEN][WALLPAPER].as_str(), Some("/old.png"));
    assert!(!root.join(DMS_RECEIPT).exists());
    assert!(!root.join(NOCTALIA_RECEIPT).exists());
}

#[test]
fn noctalia_restores_field() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state/settings.toml");
    let root = dir.path().join("receipts");
    let first = dir.path().join("first.png");
    let second = dir.path().join("second.png");
    setup(
        &path,
        "config_version = 1\n[theme]\nmode = 'dark'\n[lockscreen]\nwallpaper = '/old.png' # chosen\nblurred_desktop = true # capture\n",
    );
    noctalia_at(&noctalia_config(), Some(&first), &root, &path).unwrap();
    let applied: DocumentMut = std::fs::read_to_string(&path).unwrap().parse().unwrap();
    assert_eq!(applied[LOCKSCREEN][WALLPAPER].as_str(), first.to_str());
    assert_eq!(applied[LOCKSCREEN][BLURRED_DESKTOP].as_bool(), Some(false));
    assert_eq!(applied["theme"]["mode"].as_str(), Some("dark"));
    noctalia_at(&noctalia_config(), Some(&second), &root, &path).unwrap();
    noctalia_at(&off_config(), None, &root, &path).unwrap();
    let restored: DocumentMut = std::fs::read_to_string(&path).unwrap().parse().unwrap();
    assert_eq!(restored[LOCKSCREEN][WALLPAPER].as_str(), Some("/old.png"));
    assert_eq!(restored[LOCKSCREEN][BLURRED_DESKTOP].as_bool(), Some(true));
    assert_eq!(restored["theme"]["mode"].as_str(), Some("dark"));
    assert!(std::fs::read_to_string(&path).unwrap().contains("# chosen"));
    assert!(std::fs::read_to_string(&path).unwrap().contains("# capture"));
    assert!(!root.join(NOCTALIA_RECEIPT).exists());
}

#[test]
fn noctalia_recovers_first_write() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state/settings.toml");
    let root = dir.path().join("receipts");
    let image = dir.path().join("first.png");
    setup(
        &path,
        "config_version = 1\n[lockscreen]\nwallpaper = '/old.png'\nblurred_desktop = true\n",
    );
    let receipt = Receipt {
        capture_before: Some(true),
        capture_owned: true,
        ..Receipt::new(&path, Some("/old.png".to_owned()), image.to_string_lossy().into_owned())
    };
    save(&root.join(NOCTALIA_RECEIPT), &receipt).unwrap();
    noctalia_at(&noctalia_config(), Some(&image), &root, &path).unwrap();
    let applied: DocumentMut = std::fs::read_to_string(&path).unwrap().parse().unwrap();
    assert_eq!(applied[LOCKSCREEN][WALLPAPER].as_str(), image.to_str());
    assert_eq!(applied[LOCKSCREEN][BLURRED_DESKTOP].as_bool(), Some(false));
    noctalia_at(&off_config(), None, &root, &path).unwrap();
    let restored: DocumentMut = std::fs::read_to_string(&path).unwrap().parse().unwrap();
    assert_eq!(restored[LOCKSCREEN][WALLPAPER].as_str(), Some("/old.png"));
    assert_eq!(restored[LOCKSCREEN][BLURRED_DESKTOP].as_bool(), Some(true));
}

#[test]
fn noctalia_respects_edits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state/settings.toml");
    let root = dir.path().join("receipts");
    let image = dir.path().join("first.png");
    setup(&path, "config_version = 1\n[theme]\nmode = 'dark'\n");
    noctalia_at(&noctalia_config(), Some(&image), &root, &path).unwrap();
    setup(
        &path,
        "config_version = 1\n[theme]\nmode = 'light'\n[lockscreen]\nwallpaper = '/user.png'\n",
    );
    assert!(noctalia_at(&noctalia_config(), Some(&image), &root, &path).is_err());
    noctalia_at(&off_config(), None, &root, &path).unwrap();
    let result: DocumentMut = std::fs::read_to_string(&path).unwrap().parse().unwrap();
    assert_eq!(result[LOCKSCREEN][WALLPAPER].as_str(), Some("/user.png"));
    assert!(result[LOCKSCREEN].get(BLURRED_DESKTOP).is_none());
    assert_eq!(result["theme"]["mode"].as_str(), Some("light"));
}

#[test]
fn noctalia_respects_capture_edit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state/settings.toml");
    let root = dir.path().join("receipts");
    let image = dir.path().join("first.png");
    let second = dir.path().join("second.png");
    setup(&path, "config_version = 1\n[lockscreen]\nwallpaper = '/old.png'\n");
    noctalia_at(&noctalia_config(), Some(&image), &root, &path).unwrap();
    let mut edited: DocumentMut = std::fs::read_to_string(&path).unwrap().parse().unwrap();
    edited[LOCKSCREEN][BLURRED_DESKTOP] = toml_edit::value(true);
    setup(&path, &edited.to_string());
    noctalia_at(&noctalia_config(), Some(&second), &root, &path).unwrap();
    let updated: DocumentMut = std::fs::read_to_string(&path).unwrap().parse().unwrap();
    assert_eq!(updated[LOCKSCREEN][WALLPAPER].as_str(), second.to_str());
    assert_eq!(updated[LOCKSCREEN][BLURRED_DESKTOP].as_bool(), Some(true));
    noctalia_at(&off_config(), None, &root, &path).unwrap();
    let result: DocumentMut = std::fs::read_to_string(&path).unwrap().parse().unwrap();
    assert_eq!(result[LOCKSCREEN][WALLPAPER].as_str(), Some("/old.png"));
    assert_eq!(result[LOCKSCREEN][BLURRED_DESKTOP].as_bool(), Some(true));
}

#[test]
fn noctalia_clears_override() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state/settings.toml");
    let root = dir.path().join("receipts");
    let image = dir.path().join("first.png");
    setup(&path, "config_version = 1\n[theme]\nmode = 'dark'\n");
    noctalia_at(&noctalia_config(), Some(&image), &root, &path).unwrap();
    let applied: DocumentMut = std::fs::read_to_string(&path).unwrap().parse().unwrap();
    assert_eq!(applied[LOCKSCREEN][BLURRED_DESKTOP].as_bool(), Some(false));
    noctalia_at(&off_config(), None, &root, &path).unwrap();
    let restored: DocumentMut = std::fs::read_to_string(&path).unwrap().parse().unwrap();
    assert!(restored.get(LOCKSCREEN).is_none());
    assert_eq!(restored["theme"]["mode"].as_str(), Some("dark"));
}

#[test]
fn noctalia_rejects_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state/settings.toml");
    let root = dir.path().join("receipts");
    let image = dir.path().join("first.png");
    for text in ["{broken", "config_version = 15\n"] {
        setup(&path, text);
        assert!(noctalia_at(&noctalia_config(), Some(&image), &root, &path).is_err());
        assert!(!root.join(NOCTALIA_RECEIPT).exists());
    }
    setup(&path, "config_version = 1\n[lockscreen]\nblurred_desktop = 'bad'\n");
    assert!(noctalia_at(&noctalia_config(), Some(&image), &root, &path).is_err());
    setup(&path, "config_version = 14\n[lockscreen]\nblurred_desktop = true\n");
    noctalia_at(&noctalia_config(), Some(&image), &root, &path).unwrap();
    let result: DocumentMut = std::fs::read_to_string(&path).unwrap().parse().unwrap();
    assert_eq!(result[LOCKSCREEN][BLURRED_DESKTOP].as_bool(), Some(false));
}

#[cfg(unix)]
#[test]
fn shells_reject_symlinks() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.json");
    let link = dir.path().join("config/settings.json");
    let root = dir.path().join("receipts");
    let image = dir.path().join("first.png");
    setup(&source, "{}");
    std::fs::create_dir_all(link.parent().unwrap()).unwrap();
    symlink(&source, &link).unwrap();
    assert!(dms_at(&dms_config(), Some(&image), &root, &link).is_err());
    assert_eq!(std::fs::read_to_string(source).unwrap(), "{}");
    assert!(!root.join(DMS_RECEIPT).exists());
    let source = dir.path().join("source.toml");
    let link = dir.path().join("state/settings.toml");
    setup(&source, "config_version = 1\n");
    std::fs::create_dir_all(link.parent().unwrap()).unwrap();
    symlink(&source, &link).unwrap();
    assert!(noctalia_at(&noctalia_config(), Some(&image), &root, &link).is_err());
    assert_eq!(std::fs::read_to_string(source).unwrap(), "config_version = 1\n");
    assert!(!root.join(NOCTALIA_RECEIPT).exists());
}

#[test]
fn shells_keep_full_backups() {
    for (name, original, config) in [
        ("settings.json", "{\n  \"theme\": \"keep\"\n}\n", dms_config()),
        ("settings.toml", "# keep formatting\nconfig_version = 14\n", noctalia_config()),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        let root = dir.path().join("receipts");
        let image = dir.path().join("image.png");
        setup(&path, original);
        let sync = if name.ends_with("json") { dms_at } else { noctalia_at };
        sync(&config, Some(&image), &root, &path).unwrap();
        sync(&config, Some(&image), &root, &path).unwrap();
        sync(&off_config(), None, &root, &path).unwrap();
        assert_eq!(
            std::fs::read(dir.path().join(format!("{name}.skwd-backup"))).unwrap(),
            original.as_bytes()
        );
        assert!(!dir.path().join(format!("{name}.skwd-backup.1")).exists());
        sync(&config, Some(&image), &root, &path).unwrap();
        assert!(dir.path().join(format!("{name}.skwd-backup.1")).exists());
    }
}

#[test]
fn shells_refresh_orphan_backups() {
    for disable in [false, true] {
        for (name, record, original, edited, config) in [
            (
                "settings.json",
                DMS_RECEIPT,
                "{\"theme\":\"old\"}",
                "{\"theme\":\"new\"}",
                dms_config(),
            ),
            ("settings.toml", NOCTALIA_RECEIPT, "# old\n", "# new\n", noctalia_config()),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join(name);
            let root = dir.path().join("receipts");
            let image = dir.path().join("image.png");
            setup(&path, original);
            super::super::backup::begin(&path, &root.join(record)).unwrap();
            let sync = if name.ends_with("json") { dms_at } else { noctalia_at };
            if disable {
                sync(&off_config(), None, &root, &path).unwrap();
                assert!(!super::super::backup::marker(&root.join(record)).exists());
            }
            setup(&path, edited);
            sync(&config, Some(&image), &root, &path).unwrap();
            assert_eq!(
                std::fs::read(dir.path().join(format!("{name}.skwd-backup.1"))).unwrap(),
                edited.as_bytes()
            );
            sync(&config, Some(&image), &root, &path).unwrap();
            assert!(!dir.path().join(format!("{name}.skwd-backup.2")).exists());
        }
    }
}
