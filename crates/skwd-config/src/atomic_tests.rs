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
