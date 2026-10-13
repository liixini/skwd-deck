use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};

#[test]
fn retains_exact_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("config");
    let record = dir.path().join("receipt");
    let original = b"# settings\r\n\xff\0";
    std::fs::write(&source, original).unwrap();
    begin(&source, &record).unwrap();
    let copy = source.with_file_name("config.skwd-backup");
    assert_eq!(std::fs::read(&copy).unwrap(), original);
    assert_eq!(std::fs::metadata(&copy).unwrap().permissions().mode() & 0o777, 0o600);
    std::fs::write(&source, "changed").unwrap();
    begin(&source, &record).unwrap();
    assert!(!source.with_file_name("config.skwd-backup.1").exists());
    end(&record).unwrap();
    assert_eq!(std::fs::read(&copy).unwrap(), original);
    begin(&source, &record).unwrap();
    assert_eq!(std::fs::read(source.with_file_name("config.skwd-backup.1")).unwrap(), b"changed");
}

#[test]
fn records_missing_original() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("config");
    let record = dir.path().join("receipt");
    begin(&source, &record).unwrap();
    std::fs::write(&source, "generated").unwrap();
    begin(&source, &record).unwrap();
    assert!(!source.with_file_name("config.skwd-backup").exists());
    let saved: Record = serde_json::from_slice(&std::fs::read(marker(&record)).unwrap()).unwrap();
    assert!(saved.copy.is_none());
    assert!(begin(&dir.path().join("other"), &record).is_err());
}

#[test]
fn preserves_name_collisions() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("config");
    let record = dir.path().join("receipt");
    let previous = dir.path().join("previous");
    std::fs::write(&source, "original").unwrap();
    std::fs::write(&previous, "older").unwrap();
    symlink(&previous, source.with_file_name("config.skwd-backup")).unwrap();
    begin(&source, &record).unwrap();
    assert_eq!(std::fs::read(&previous).unwrap(), b"older");
    assert_eq!(std::fs::read(source.with_file_name("config.skwd-backup.1")).unwrap(), b"original");
}

#[test]
fn refuses_unsafe_paths() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("config");
    let record = dir.path().join("receipt");
    let original = dir.path().join("original");
    std::fs::write(&original, "original").unwrap();
    symlink(&original, &source).unwrap();
    assert!(begin(&source, &record).is_err());
    std::fs::remove_file(&source).unwrap();
    std::fs::write(&source, "original").unwrap();
    symlink(&original, marker(&record)).unwrap();
    assert!(begin(&source, &record).is_err());
    assert_eq!(std::fs::read(&original).unwrap(), b"original");
    assert!(!source.with_file_name("config.skwd-backup").exists());
}

#[test]
fn backup_failure_blocks_edit() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("config");
    let record = dir.path().join("receipt");
    std::fs::write(&source, "original").unwrap();
    std::fs::create_dir(marker(&record)).unwrap();
    let result = ownership::manage(&source, &record, true, |_| {
        Ok(vec![ownership::Patch {
            before: "original".into(),
            after: "changed".into(),
            retained_section: None,
        }])
    });
    assert!(result.is_err());
    assert_eq!(std::fs::read(&source).unwrap(), b"original");
    assert!(!record.exists());
}

#[test]
fn refuses_readonly_source() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("config");
    let record = dir.path().join("receipt");
    std::fs::write(&source, "original").unwrap();
    std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o400)).unwrap();
    assert!(begin(&source, &record).is_err());
    assert!(!marker(&record).exists());
    assert!(!source.with_file_name("config.skwd-backup").exists());
}

#[test]
fn legacy_disable_copies_current() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("config");
    let record = dir.path().join("receipt");
    std::fs::write(&source, "original").unwrap();
    ownership::manage(&source, &record, true, |_| {
        Ok(vec![ownership::Patch {
            before: "original".into(),
            after: "changed".into(),
            retained_section: None,
        }])
    })
    .unwrap();
    std::fs::remove_file(marker(&record)).unwrap();
    std::fs::remove_file(source.with_file_name("config.skwd-backup")).unwrap();
    ownership::manage(&source, &record, false, |_| unreachable!()).unwrap();
    assert_eq!(std::fs::read(source.with_file_name("config.skwd-backup")).unwrap(), b"changed");
    assert_eq!(std::fs::read(&source).unwrap(), b"original");
    assert!(!marker(&record).exists());
}

#[test]
fn damaged_backup_blocks_edit() {
    for corrupt in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("config");
        let record = dir.path().join("receipt");
        std::fs::write(&source, "original").unwrap();
        begin(&source, &record).unwrap();
        if corrupt {
            std::fs::write(marker(&record), "invalid").unwrap();
        } else {
            std::fs::remove_file(source.with_file_name("config.skwd-backup")).unwrap();
        }
        assert!(ownership::manage(&source, &record, true, |_| unreachable!()).is_err());
        assert_eq!(std::fs::read(&source).unwrap(), b"original");
    }
}

#[test]
fn failed_setup_refreshes_backup() {
    for fail_save in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("config");
        let record = dir.path().join("receipt");
        std::fs::write(&source, "original").unwrap();
        assert!(
            ownership::manage(&source, &record, true, |_| {
                if !fail_save {
                    anyhow::bail!("Invalid patch");
                }
                symlink(&source, &record).unwrap();
                Ok(vec![ownership::Patch {
                    before: "original".into(),
                    after: "changed".into(),
                    retained_section: None,
                }])
            })
            .is_err()
        );
        if fail_save {
            std::fs::remove_file(&record).unwrap();
        }
        std::fs::write(&source, "edited").unwrap();
        ownership::manage(&source, &record, true, |_| {
            Ok(vec![ownership::Patch {
                before: "edited".into(),
                after: "changed".into(),
                retained_section: None,
            }])
        })
        .unwrap();
        assert_eq!(
            std::fs::read(source.with_file_name("config.skwd-backup")).unwrap(),
            b"original"
        );
        assert_eq!(
            std::fs::read(source.with_file_name("config.skwd-backup.1")).unwrap(),
            b"edited"
        );
    }
}
