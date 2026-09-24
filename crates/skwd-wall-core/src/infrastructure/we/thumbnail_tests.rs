use super::*;

#[test]
fn preset_capture_is_invalidated_by_parent_changes() {
    let root = tempfile::tempdir().unwrap();
    let base = root.path().join("1");
    let preset = root.path().join("2");
    std::fs::create_dir(&base).unwrap();
    std::fs::create_dir(&preset).unwrap();
    std::fs::write(base.join("project.json"), b"{}").unwrap();
    std::fs::write(base.join("scene.pkg"), b"fixture").unwrap();
    std::fs::write(preset.join("project.json"), br#"{"dependency":"1","preset":{}}"#).unwrap();
    let original = signature(&preset, &serde_json::Map::new()).unwrap();
    std::fs::write(base.join("scene.pkg"), b"changed parent").unwrap();
    assert_ne!(original, signature(&preset, &serde_json::Map::new()).unwrap());
    std::fs::remove_file(base.join("project.json")).unwrap();
    assert!(signature(&preset, &serde_json::Map::new()).is_err());
}

fn fixture() -> (tempfile::TempDir, serde_json::Value, Vec<PathBuf>, serde_json::Value) {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("scene");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("project.json"), b"{}").unwrap();
    std::fs::write(source.join("scene.pkg"), b"package").unwrap();
    let artifacts: Vec<_> = ["thumb.webp", "small.webp", "near.bc7", "far.bc1"]
        .iter()
        .map(|name| {
            let path = root.path().join(name);
            std::fs::write(&path, b"rendered thumbnail").unwrap();
            path
        })
        .collect();
    let input = signature(&source, &serde_json::Map::new()).unwrap();
    let record =
        serde_json::json!({"input": input, "artifacts": artifact_stamps(&artifacts).unwrap()});
    (root, record, artifacts, input)
}

#[test]
fn unchanged_scene_reuses_persisted_capture() {
    let (root, stamp, artifacts, input) = fixture();
    assert!(cached(Some(&stamp), &input, &artifacts));
    let reloaded = signature(&root.path().join("scene"), &serde_json::Map::new()).unwrap();
    assert!(cached(Some(&stamp), &reloaded, &artifacts));
}

#[test]
fn scene_files_and_properties_invalidate_capture() {
    let (root, stamp, artifacts, input) = fixture();
    let source = root.path().join("scene");
    let properties = serde_json::json!({"color": "1 0 0"}).as_object().unwrap().clone();
    assert!(!cached(Some(&stamp), &signature(&source, &properties).unwrap(), &artifacts));
    std::fs::write(source.join("scene.pkg"), b"changed package").unwrap();
    let changed = signature(&source, &serde_json::Map::new()).unwrap();
    assert_ne!(input, changed);
    assert!(!cached(Some(&stamp), &changed, &artifacts));
}

#[test]
fn loose_asset_changes_and_removal_invalidate_capture() {
    let (root, _, _, _) = fixture();
    let source = root.path().join("scene");
    let baseline = signature(&source, &serde_json::Map::new()).unwrap();
    std::fs::create_dir(source.join("materials")).unwrap();
    std::fs::write(source.join("materials/custom.json"), b"{}").unwrap();
    let added = signature(&source, &serde_json::Map::new()).unwrap();
    assert_ne!(baseline, added);
    std::fs::remove_file(source.join("materials/custom.json")).unwrap();
    assert_ne!(added, signature(&source, &serde_json::Map::new()).unwrap());
}

#[test]
fn missing_or_replaced_artifact_requires_capture() {
    for index in 0..4 {
        let (_root, stamp, artifacts, input) = fixture();
        std::fs::write(&artifacts[index], b"scanner replaced preview").unwrap();
        assert!(!cached(Some(&stamp), &input, &artifacts));
        std::fs::remove_file(&artifacts[index]).unwrap();
        assert!(!cached(Some(&stamp), &input, &artifacts));
    }
}

#[test]
fn failed_or_unrecorded_capture_can_retry() {
    let (_root, _stamp, artifacts, input) = fixture();
    assert!(!cached(None, &input, &artifacts));
    assert!(!cached(Some(&serde_json::Value::Null), &input, &artifacts));
}

#[test]
fn parallel_monitors_serialize_on_the_scene_lock() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("scene.lock");
    let first = std::fs::File::create(&path).unwrap();
    let second = std::fs::File::options().write(true).open(&path).unwrap();
    first.lock().unwrap();
    assert!(matches!(second.try_lock(), Err(std::fs::TryLockError::WouldBlock)));
    drop(first);
    assert!(second.try_lock().is_ok());
}
