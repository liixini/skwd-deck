use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

use super::*;

fn fixture() -> (tempfile::TempDir, Paths) {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths { root: dir.path().to_owned(), owner: unsafe { libc::geteuid() } };
    let config = paths.root.join("etc/sddm.conf");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(config, "[Theme]\nCurrent=breeze\n").unwrap();
    let theme = paths.root.join("usr/share/sddm/themes/breeze");
    std::fs::create_dir_all(&theme).unwrap();
    std::fs::write(theme.join("metadata.desktop"), "[SddmGreeterTheme]\nConfigFile=theme.conf\n")
        .unwrap();
    std::fs::write(theme.join("theme.conf"), "[General]\nbackground=original.png\ntype=image\n")
        .unwrap();
    (dir, paths)
}

#[test]
fn setup_restore_repeat() {
    let (_dir, paths) = fixture();
    let uid = paths.owner;
    let config = paths.root.join("usr/share/sddm/themes/breeze/theme.conf.user");
    let original = "[General]\nbackground=old.png\ntype=color\nshowClock=true\n";
    std::fs::write(&config, original).unwrap();
    let image = sync(&paths, uid, true, None).unwrap();
    assert!(!image.exists());
    assert_eq!(std::fs::metadata(image.parent().unwrap()).unwrap().mode() & 0o777, 0o755);
    let installed = std::fs::read_to_string(&config).unwrap();
    assert!(installed.contains(&format!("background={}\n", image.display())));
    std::fs::write(&config, installed.replace("showClock=true", "showClock=false")).unwrap();
    sync(&paths, uid, false, None).unwrap();
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        original.replace("showClock=true", "showClock=false")
    );
    sync(&paths, uid, true, None).unwrap();
    sync(&paths, uid, false, None).unwrap();
}

#[test]
fn rejects_changed_background() {
    let (_dir, paths) = fixture();
    let uid = paths.owner;
    sync(&paths, uid, true, None).unwrap();
    let config = paths.root.join("usr/share/sddm/themes/breeze/theme.conf.user");
    std::fs::write(&config, "[General]\nbackground=custom.png\ntype=image\n").unwrap();
    assert!(sync(&paths, uid, false, None).is_err());
    assert!(std::fs::read_to_string(config).unwrap().contains("custom.png"));
}

#[test]
fn rejects_other_user() {
    let (_dir, paths) = fixture();
    sync(&paths, paths.owner, true, None).unwrap();
    assert!(sync(&paths, paths.owner + 1, true, None).is_err());
    assert!(sync(&paths, paths.owner + 1, false, None).is_err());
}

#[test]
fn rejects_embedded_theme() {
    let (_dir, paths) = fixture();
    std::fs::write(paths.root.join("etc/sddm.conf"), "[Theme]\nCurrent=\n").unwrap();
    assert!(sync(&paths, paths.owner, true, None).unwrap_err().to_string().contains("embedded"));
    assert!(!paths.root.join(DATA).join(RECEIPT).exists());
}

#[test]
fn rejects_untrusted_paths() {
    for symlinked in [false, true] {
        let (_dir, paths) = fixture();
        let theme = paths.root.join("usr/share/sddm/themes/breeze");
        if symlinked {
            let moved = paths.root.join("theme");
            std::fs::rename(&theme, &moved).unwrap();
            symlink(moved, &theme).unwrap();
        } else {
            std::fs::set_permissions(&theme, std::fs::Permissions::from_mode(0o777)).unwrap();
        }
        assert!(sync(&paths, paths.owner, true, None).is_err());
        assert!(!paths.root.join(DATA).join(RECEIPT).exists());
    }
}

#[test]
fn restores_created_override() {
    let (_dir, paths) = fixture();
    sync(&paths, paths.owner, true, None).unwrap();
    sync(&paths, paths.owner, false, None).unwrap();
    assert!(!paths.root.join("usr/share/sddm/themes/breeze/theme.conf.user").exists());
}

#[test]
fn rolls_back_invalid_override() {
    let (_dir, paths) = fixture();
    let config = paths.root.join("usr/share/sddm/themes/breeze/theme.conf.user");
    let invalid = "[General]\nbackground=one\nbackground=two\n";
    std::fs::write(&config, invalid).unwrap();
    assert!(sync(&paths, paths.owner, true, None).is_err());
    assert_eq!(std::fs::read_to_string(config).unwrap(), invalid);
    assert!(!paths.root.join(DATA).join(RECEIPT).exists());
}

#[test]
fn honors_config_order() {
    let (_dir, paths) = fixture();
    let dir = paths.root.join("etc/sddm.conf.d");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("zz-theme.conf"), "[Theme]\nCurrent=unsupported\n").unwrap();
    assert!(
        settings::theme(&paths, &settings::selection(&paths).unwrap().0)
            .unwrap()
            .ends_with("breeze/theme.conf.user")
    );
    std::fs::remove_file(paths.root.join("etc/sddm.conf")).unwrap();
    assert!(settings::theme(&paths, &settings::selection(&paths).unwrap().0).is_err());
}

#[test]
fn rejects_commented_duplicate() {
    let (_dir, paths) = fixture();
    let config = paths.root.join("usr/share/sddm/themes/breeze/theme.conf.user");
    let original = "[General]\nbackground=old.png\n[General] ; alternate\nbackground=secret.png\n";
    std::fs::write(&config, original).unwrap();
    assert!(sync(&paths, paths.owner, true, None).is_err());
    assert_eq!(std::fs::read_to_string(config).unwrap(), original);
}

#[test]
fn rejects_equivalent_sections() {
    for name in ["general", " General "] {
        let (_dir, paths) = fixture();
        let config = paths.root.join("usr/share/sddm/themes/breeze/theme.conf.user");
        let original = format!("[General]\nbackground=old.png\n[{name}]\nbackground=secret.png\n");
        std::fs::write(&config, &original).unwrap();
        assert!(sync(&paths, paths.owner, true, None).is_err());
        assert_eq!(std::fs::read_to_string(config).unwrap(), original);
    }
}

#[test]
fn preserves_utf8_marker() {
    let (_dir, paths) = fixture();
    let config = paths.root.join("usr/share/sddm/themes/breeze/theme.conf.user");
    let original = "\u{feff}[General]\nbackground=old.png\n";
    std::fs::write(&config, original).unwrap();
    sync(&paths, paths.owner, true, None).unwrap();
    sync(&paths, paths.owner, false, None).unwrap();
    assert_eq!(std::fs::read_to_string(config).unwrap(), original);
}

#[test]
fn rejects_multiline_values() {
    let (_dir, paths) = fixture();
    let config = paths.root.join("usr/share/sddm/themes/breeze/theme.conf.user");
    let original = "[General]\nshowClock=\"foo\nbackground=secret.png\n\"\n";
    std::fs::write(&config, original).unwrap();
    assert!(sync(&paths, paths.owner, true, None).is_err());
    assert_eq!(std::fs::read_to_string(config).unwrap(), original);
}

#[test]
fn supports_commented_section() {
    let (_dir, paths) = fixture();
    let config = paths.root.join("usr/share/sddm/themes/breeze/theme.conf.user");
    let original = "[General] ; appearance\nbackground=old.png\n";
    std::fs::write(&config, original).unwrap();
    sync(&paths, paths.owner, true, None).unwrap();
    sync(&paths, paths.owner, false, None).unwrap();
    assert_eq!(std::fs::read_to_string(config).unwrap(), original);
}

#[test]
fn preserves_added_setting() {
    let (_dir, paths) = fixture();
    let config = paths.root.join("usr/share/sddm/themes/breeze/theme.conf.user");
    std::fs::write(&config, "[General]\nshowClock=true\n").unwrap();
    sync(&paths, paths.owner, true, None).unwrap();
    let installed = std::fs::read_to_string(&config).unwrap();
    std::fs::write(&config, installed.replace("[General]\n", "[General]\nClockFont=Roboto\n"))
        .unwrap();
    sync(&paths, paths.owner, false, None).unwrap();
    assert_eq!(
        std::fs::read_to_string(config).unwrap(),
        "[General]\nClockFont=Roboto\nshowClock=true\n"
    );
}

#[test]
fn explicit_theme_restore() {
    let (_dir, paths) = fixture();
    let config = paths.root.join("etc/sddm.conf");
    let original = "[Theme]\nCurrent=\nCursorSize=24\n";
    std::fs::write(&config, original).unwrap();
    assert_eq!(settings::choices(&paths).unwrap().themes, ["breeze"]);
    sync(&paths, paths.owner, true, Some("breeze")).unwrap();
    assert!(std::fs::read_to_string(&config).unwrap().contains("Current=breeze"));
    sync(&paths, paths.owner, false, None).unwrap();
    assert_eq!(std::fs::read_to_string(config).unwrap(), original);
}

#[test]
fn preserves_original_modes() {
    let (_dir, paths) = fixture();
    let config = paths.root.join("usr/share/sddm/themes/breeze/theme.conf.user");
    std::fs::write(&config, "[General]\nbackground=old.png\n").unwrap();
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o640)).unwrap();
    sync(&paths, paths.owner, true, None).unwrap();
    sync(&paths, paths.owner, false, None).unwrap();
    assert_eq!(std::fs::metadata(config).unwrap().mode() & 0o777, 0o640);
}

#[test]
fn rejects_export_symlink() {
    let (_dir, paths) = fixture();
    let image = sync(&paths, paths.owner, true, None).unwrap();
    let directory = image.parent().unwrap();
    std::fs::remove_dir(directory).unwrap();
    symlink(paths.root.join("etc"), directory).unwrap();
    assert!(sync(&paths, paths.owner, true, None).is_err());
    assert!(!paths.root.join("etc/wallpaper.png").exists());
}

#[test]
fn repeats_explicit_selection() {
    let (_dir, paths) = fixture();
    let config = paths.root.join("etc/sddm.conf");
    std::fs::write(&config, "[Theme]\nCurrent=\n").unwrap();
    sync(&paths, paths.owner, true, Some("breeze")).unwrap();
    sync(&paths, paths.owner, true, None).unwrap();
    sync(&paths, paths.owner, false, None).unwrap();
    assert_eq!(std::fs::read_to_string(config).unwrap(), "[Theme]\nCurrent=\n");
}

#[test]
fn rolls_back_selection_failure() {
    let (_dir, paths) = fixture();
    let config = paths.root.join("etc/sddm.conf");
    std::fs::write(&config, "[Theme]\nCurrent=\n").unwrap();
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o444)).unwrap();
    assert!(sync(&paths, paths.owner, true, Some("breeze")).is_err());
    assert!(!paths.root.join("usr/share/sddm/themes/breeze/theme.conf.user").exists());
    assert!(!paths.root.join(DATA).join(RECEIPT).exists());
}

#[test]
fn retains_added_section_fields() {
    let (_dir, paths) = fixture();
    let system = paths.root.join("etc/sddm.conf");
    std::fs::remove_file(&system).unwrap();
    let image = sync(&paths, paths.owner, true, Some("breeze")).unwrap();
    let config = paths.root.join("usr/share/sddm/themes/breeze/theme.conf.user");
    std::fs::write(&system, "[Theme]\nCursorSize=24\nCurrent=breeze\n").unwrap();
    std::fs::write(
        &config,
        format!("[General]\nbackground={}\nshowClock=false\ntype=image\n", image.display()),
    )
    .unwrap();
    sync(&paths, paths.owner, false, None).unwrap();
    assert_eq!(std::fs::read_to_string(system).unwrap(), "[Theme]\nCursorSize=24\n");
    assert_eq!(std::fs::read_to_string(config).unwrap(), "[General]\nshowClock=false\n");
}

#[test]
fn backups_precede_config_edits() {
    let (_dir, paths) = fixture();
    let config = paths.root.join("usr/share/sddm/themes/breeze/theme.conf.user");
    let original = "[General]\nbackground=old.png\ntype=color\n";
    std::fs::write(&config, original).unwrap();
    let marker =
        super::super::super::backup::marker(&paths.root.join(DATA).join(SELECTION_PATCHES));
    std::fs::create_dir_all(marker).unwrap();
    assert!(sync(&paths, paths.owner, true, Some("breeze")).is_err());
    assert_eq!(std::fs::read(&config).unwrap(), original.as_bytes());
    assert_eq!(
        std::fs::read(paths.root.join("etc/sddm.conf")).unwrap(),
        b"[Theme]\nCurrent=breeze\n"
    );
    let copy = config.with_file_name("theme.conf.user.skwd-backup");
    assert_eq!(std::fs::read(&copy).unwrap(), original.as_bytes());
    let meta = std::fs::metadata(copy).unwrap();
    assert_eq!(meta.mode() & 0o777, 0o600);
    assert_eq!(meta.uid(), paths.owner);
}

#[test]
fn selection_backup_survives_disable() {
    let (_dir, paths) = fixture();
    let system = paths.root.join("etc/sddm.conf");
    let original = std::fs::read(&system).unwrap();
    sync(&paths, paths.owner, true, Some("breeze")).unwrap();
    sync(&paths, paths.owner, false, None).unwrap();
    assert_eq!(std::fs::read(system.with_file_name("sddm.conf.skwd-backup")).unwrap(), original);
}
