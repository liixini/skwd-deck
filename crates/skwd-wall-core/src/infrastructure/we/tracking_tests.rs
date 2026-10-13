use std::collections::HashMap;

use super::SteamTracking;

#[test]
fn hides_matching_folder() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("workshop");
    let native = root.path().join("native");
    let manual = root.path().join("manual");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&native).unwrap();
    std::fs::create_dir_all(&manual).unwrap();
    std::os::unix::fs::symlink(&native, source.join("42")).unwrap();
    let tracking = SteamTracking {
        source: source.clone(),
        seen: HashMap::from([("42".into(), native.clone())]),
        hidden: HashMap::from([("42".into(), native)]),
        ..SteamTracking::default()
    };
    assert!(tracking.hides(&source, "42", &source.join("42")));
    assert!(!tracking.hides(&source, "42", &manual));
    assert!(!tracking.hides(&manual, "42", &source.join("42")));
    assert!(tracking.subscribed(&source, "42", &source.join("42")));
    assert!(!tracking.subscribed(&source, "42", &manual));
}
