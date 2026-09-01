#![cfg(test)]

use super::{sweep_effects_previews, within_dir};
use crate::testenv::tmp;
use serde_json::json;
use std::path::Path;

#[test]
fn sweep_previews_only() {
    let dir = std::env::temp_dir().join(format!("skwd-test-sweep-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("123-a.png"), b"x").unwrap();
    std::fs::write(dir.join("456-b.png"), b"x").unwrap();
    std::fs::write(dir.join("789-c.webp"), b"x").unwrap();
    std::fs::write(dir.join("keep.txt"), b"x").unwrap();
    assert_eq!(sweep_effects_previews(&dir), 3);
    assert!(!dir.join("123-a.png").exists());
    assert!(!dir.join("789-c.webp").exists());
    assert!(dir.join("keep.txt").exists());
    assert_eq!(sweep_effects_previews(&dir), 0);
    assert_eq!(sweep_effects_previews(Path::new("/nonexistent-skwd")), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn within_dir_traversal() {
    let tmp = tempfile::tempdir().unwrap();
    let walls = tmp.path().join("walls");
    let walls2 = tmp.path().join("walls2");
    std::fs::create_dir_all(walls.join("sub")).unwrap();
    std::fs::create_dir_all(&walls2).unwrap();
    std::fs::write(walls.join("sub/ok.png"), b"x").unwrap();
    std::fs::write(walls2.join("esc.png"), b"x").unwrap();
    assert!(within_dir(&walls.join("sub/ok.png"), &walls));
    assert!(!within_dir(&walls2.join("esc.png"), &walls));
    assert!(!within_dir(&walls.join("sub/../../walls2/esc.png"), &walls));
    assert!(!within_dir(Path::new("/nonexistent-skwd/x.png"), &walls));
}

#[test]
fn commit_outside_rejected() {
    let base = tmp("fx");
    let walls = base.join("walls");
    std::fs::create_dir_all(&walls).unwrap();
    let outside = base.join("outside.png");
    std::fs::write(&outside, b"x").unwrap();
    let err = super::effects_commit(
        &outside.to_string_lossy(),
        &json!([{ "effect": "grayscale", "params": {} }]),
        &walls.to_string_lossy(),
        "/nonexistent-skwd-videos",
    )
    .expect_err("commit confined");
    assert!(err.to_string().contains("outside"));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn requested_effects_stack() {
    let legacy = super::requested_effects("invert", &json!({}), None);
    assert_eq!(legacy, json!([{"effect": "invert", "params": {}}]));

    let stack = json!([
        {"effect": "theme", "params": {"theme": "Nord"}},
        {"effect": "kuwahara", "params": {"radius": 8}},
    ]);
    assert_eq!(super::requested_effects("invert", &json!({}), Some(&stack)), stack);
    assert_eq!(super::effect_chain_tag_label(&stack), "nord,kuwahara");
    assert_eq!(super::effect_chain_suffix(&stack), "theme-nord-kuwahara");
}

#[test]
fn long_stack_suffix() {
    let stack = serde_json::Value::Array(
        (0..64).map(|index| json!({"effect": format!("effect-{index}"), "params": {}})).collect(),
    );
    let suffix = super::effect_chain_suffix(&stack);
    assert!(suffix.len() < 180);
    assert!(suffix.contains("-stack-"));
}
