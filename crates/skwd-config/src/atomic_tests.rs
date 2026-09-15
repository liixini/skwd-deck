#![cfg(test)]

use super::*;

#[cfg(unix)]
#[test]
fn write_mode_sets_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("secret.json");
    atomic_write_mode(&path, b"{}", Some(0o600)).unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    assert_eq!(std::fs::read(&path).unwrap(), b"{}");
    let leftover = std::fs::read_dir(directory.path())
        .unwrap()
        .any(|entry| entry.unwrap().file_name().to_string_lossy().contains(".tmp"));
    assert!(!leftover);
}

#[test]
fn write_replaces_existing_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("colors.json");
    atomic_write(&path, br##"{"primary":"#111111"}"##).unwrap();
    atomic_write(&path, br##"{"primary":"#222222"}"##).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), r##"{"primary":"#222222"}"##);
    let leftovers: Vec<_> = std::fs::read_dir(directory.path())
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
fn write_survives_concurrent_writers() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("colors.json");
    std::thread::scope(|scope| {
        for writer in 0..8 {
            let target = target.clone();
            scope.spawn(move || {
                let payload = format!(r#"{{"writer":{writer}}}"#);
                atomic_write(&target, payload.as_bytes()).unwrap();
            });
        }
    });
    let text = std::fs::read_to_string(&target).unwrap();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(value.get("writer").is_some());
    let leftovers: Vec<_> = std::fs::read_dir(directory.path())
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
fn failed_write_leaves_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("missing-dir");
    let target = missing.join("out.json");
    assert!(atomic_write(&target, b"{}").is_err());
    assert!(!missing.exists());
}

#[cfg(unix)]
fn has_tmp(directory: &std::path::Path) -> bool {
    std::fs::read_dir(directory)
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
}

#[cfg(unix)]
fn linked(root: &std::path::Path, target: &std::path::Path) -> std::path::PathBuf {
    let link = root.join("config.json");
    std::os::unix::fs::symlink(target, &link).unwrap();
    link
}

#[cfg(unix)]
#[test]
fn write_keeps_symlink() {
    let root = tempfile::tempdir().unwrap();
    let dots = root.path().join("dots");
    std::fs::create_dir(&dots).unwrap();
    let target = dots.join("config.json");
    std::fs::write(&target, b"{\"a\":1}").unwrap();
    let link = linked(root.path(), &target);
    atomic_write(&link, b"{\"a\":2}").unwrap();
    assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read_link(&link).unwrap(), target);
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "{\"a\":2}");
    assert_eq!(std::fs::read_to_string(&link).unwrap(), "{\"a\":2}");
    assert!(!has_tmp(root.path()));
    assert!(!has_tmp(&dots));
}

#[cfg(unix)]
#[test]
fn write_keeps_relative_symlink() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let dots = root.path().join("dots");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&dots).unwrap();
    std::fs::write(dots.join("config.json"), b"old").unwrap();
    let link = linked(&home, std::path::Path::new("../dots/config.json"));
    atomic_write(&link, b"new").unwrap();
    assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read_link(&link).unwrap(), std::path::Path::new("../dots/config.json"));
    assert_eq!(std::fs::read_to_string(dots.join("config.json")).unwrap(), "new");
    assert!(!has_tmp(&home));
    assert!(!has_tmp(&dots));
}

#[cfg(unix)]
#[test]
fn write_keeps_symlink_chain() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("real.json");
    std::fs::write(&target, b"old").unwrap();
    let middle = root.path().join("middle.json");
    std::os::unix::fs::symlink(&target, &middle).unwrap();
    let link = linked(root.path(), &middle);
    atomic_write(&link, b"new").unwrap();
    assert_eq!(std::fs::read_link(&link).unwrap(), middle);
    assert_eq!(std::fs::read_link(&middle).unwrap(), target);
    assert!(!std::fs::symlink_metadata(&target).unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
    assert!(!has_tmp(root.path()));
}

#[cfg(unix)]
#[test]
fn write_fills_dangling_symlink() {
    let root = tempfile::tempdir().unwrap();
    let dots = root.path().join("dots");
    std::fs::create_dir(&dots).unwrap();
    let target = dots.join("config.json");
    let link = linked(root.path(), &target);
    assert!(!link.exists());
    atomic_write(&link, b"{}").unwrap();
    assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "{}");
    assert!(!has_tmp(root.path()));
    assert!(!has_tmp(&dots));
}

#[cfg(unix)]
#[test]
fn write_fails_when_dangling_target_dir_is_missing() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("missing").join("config.json");
    let link = linked(root.path(), &target);
    assert!(atomic_write(&link, b"{}").is_err());
    assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert!(!root.path().join("missing").exists());
    assert!(!has_tmp(root.path()));
}

#[cfg(unix)]
#[test]
fn write_fails_on_symlink_loop() {
    let root = tempfile::tempdir().unwrap();
    let first = root.path().join("first.json");
    let second = root.path().join("second.json");
    std::os::unix::fs::symlink(&second, &first).unwrap();
    std::os::unix::fs::symlink(&first, &second).unwrap();
    assert!(atomic_write(&first, b"{}").is_err());
    assert!(std::fs::symlink_metadata(&first).unwrap().file_type().is_symlink());
    assert!(std::fs::symlink_metadata(&second).unwrap().file_type().is_symlink());
    assert!(!has_tmp(root.path()));
}

#[cfg(unix)]
#[test]
fn write_through_symlink_sets_target_mode() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("real.json");
    std::fs::write(&target, b"{}").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644)).unwrap();
    let link = linked(root.path(), &target);
    atomic_write_mode(&link, b"{\"k\":1}", Some(0o600)).unwrap();
    assert_eq!(std::fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o600);
    assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
}

#[cfg(unix)]
#[test]
fn write_through_symlink_needs_writable_target_dir() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let store = root.path().join("store");
    std::fs::create_dir(&store).unwrap();
    let target = store.join("config.json");
    std::fs::write(&target, b"old").unwrap();
    let link = linked(root.path(), &target);
    std::fs::set_permissions(&store, std::fs::Permissions::from_mode(0o500)).unwrap();
    let bypasses_permissions = std::fs::File::create(store.join("probe")).is_ok();
    let result = atomic_write(&link, b"new");
    std::fs::set_permissions(&store, std::fs::Permissions::from_mode(0o700)).unwrap();
    if bypasses_permissions {
        return;
    }
    assert!(result.is_err());
    assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "old");
    assert!(!has_tmp(root.path()));
    assert!(!has_tmp(&store));
}

#[cfg(unix)]
#[test]
fn write_through_symlink_ignores_link_dir_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let dots = root.path().join("dots");
    std::fs::create_dir(&home).unwrap();
    std::fs::create_dir(&dots).unwrap();
    let target = dots.join("config.json");
    std::fs::write(&target, b"old").unwrap();
    let link = linked(&home, &target);
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o500)).unwrap();
    let result = atomic_write(&link, b"new");
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    result.unwrap();
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
    assert!(!has_tmp(&home));
    assert!(!has_tmp(&dots));
}

#[cfg(unix)]
#[test]
fn write_inside_symlinked_directory() {
    let root = tempfile::tempdir().unwrap();
    let real = root.path().join("real");
    std::fs::create_dir(&real).unwrap();
    let alias = root.path().join("alias");
    std::os::unix::fs::symlink(&real, &alias).unwrap();
    let path = alias.join("config.json");
    atomic_write(&path, b"{}").unwrap();
    atomic_write(&path, b"{\"x\":1}").unwrap();
    assert!(std::fs::symlink_metadata(&alias).unwrap().file_type().is_symlink());
    assert!(!std::fs::symlink_metadata(&path).unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read_to_string(real.join("config.json")).unwrap(), "{\"x\":1}");
    assert!(!has_tmp(&real));
}

#[cfg(unix)]
#[test]
fn write_keeps_symlink_across_repeated_saves() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("real.json");
    let link = linked(root.path(), &target);
    for round in 0..5 {
        atomic_write(&link, format!("{round}").as_bytes()).unwrap();
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    }
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "4");
    assert!(!has_tmp(root.path()));
}

#[cfg(unix)]
#[test]
fn write_survives_concurrent_writers_through_symlink() {
    let root = tempfile::tempdir().unwrap();
    let dots = root.path().join("dots");
    std::fs::create_dir(&dots).unwrap();
    let target = dots.join("config.json");
    let link = linked(root.path(), &target);
    std::thread::scope(|scope| {
        for writer in 0..8 {
            let link = link.clone();
            scope.spawn(move || {
                let payload = format!(r#"{{"writer":{writer}}}"#);
                atomic_write(&link, payload.as_bytes()).unwrap();
            });
        }
    });
    assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&target).unwrap()).unwrap();
    assert!(value.get("writer").is_some());
    assert!(!has_tmp(root.path()));
    assert!(!has_tmp(&dots));
}

#[cfg(unix)]
#[test]
fn follow_links_leaves_plain_paths_alone() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("plain.json");
    std::fs::write(&file, b"{}").unwrap();
    assert_eq!(follow_links(&file).unwrap(), file);
    let missing = root.path().join("missing.json");
    assert_eq!(follow_links(&missing).unwrap(), missing);
}

#[cfg(unix)]
#[test]
fn follow_links_resolves_relative_chain() {
    let root = tempfile::tempdir().unwrap();
    let dots = root.path().join("dots");
    std::fs::create_dir(&dots).unwrap();
    let target = dots.join("real.json");
    std::fs::write(&target, b"{}").unwrap();
    std::os::unix::fs::symlink("real.json", dots.join("alias.json")).unwrap();
    let link = linked(root.path(), std::path::Path::new("dots/alias.json"));
    assert_eq!(follow_links(&link).unwrap(), target);
}
