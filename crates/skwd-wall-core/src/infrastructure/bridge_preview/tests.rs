#![cfg(test)]

use super::{arm, cache_key, forget, preview_end};
use crate::config::Config;
use crate::state::WallState;

#[test]
fn cache_key_mode() {
    let base = serde_json::json!({ "theme": { "backend": "skwd-iris", "style": "pastel" } });
    let mut dark = base.clone();
    dark["matugen"] = serde_json::json!({ "mode": "dark" });
    let mut light = base;
    light["matugen"] = serde_json::json!({ "mode": "light" });
    let img = "/img/a.png";
    assert_ne!(cache_key(&Config::from_root(dark), img), cache_key(&Config::from_root(light), img));
}

fn state_with_bridge(dir: &std::path::Path, body: &str) -> WallState {
    std::fs::write(dir.join("colors.json"), body).unwrap();
    WallState::test_new(serde_json::json!({
        "paths": { "cache": dir.to_string_lossy() },
    }))
}

#[test]
fn end_restores_side_files() {
    let tmp = tempfile::tempdir().unwrap();
    let kde = tmp.path().join("kde.colors");
    std::fs::write(&kde, "APPLIED").unwrap();
    std::fs::write(tmp.path().join("colors.json"), "bridge-applied").unwrap();
    let st = WallState::test_new(serde_json::json!({
        "paths": { "cache": tmp.path().to_string_lossy() },
        "integrations": [
            { "template": "bridge.json", "output": "colors.json" },
            { "template": "kde.colors", "output": kde.to_string_lossy() },
        ],
    }));

    super::arm(&st);
    std::fs::write(&kde, "HOVERED").unwrap();
    super::preview_end(&st);

    assert_eq!(std::fs::read_to_string(&kde).unwrap(), "APPLIED");
}

#[test]
fn end_restores_palette() {
    let tmp = tempfile::tempdir().unwrap();
    let st = state_with_bridge(tmp.path(), r##"{"background":"#applied"}"##);
    st.theme().arm_bridge_preview(std::fs::read(tmp.path().join("colors.json")).unwrap());
    std::fs::write(tmp.path().join("colors.json"), r##"{"background":"#hovered"}"##).unwrap();

    preview_end(&st);

    let back = std::fs::read_to_string(tmp.path().join("colors.json")).unwrap();
    assert!(back.contains("#applied"), "{back}");
    assert!(!st.theme().bridge_preview_armed());
}

#[test]
fn arm_twice_keeps_first() {
    let tmp = tempfile::tempdir().unwrap();
    let st = state_with_bridge(tmp.path(), "applied");
    st.theme().arm_bridge_preview(b"applied".to_vec());
    st.theme().arm_bridge_preview(b"first-hover".to_vec());

    assert_eq!(st.theme().take_bridge_preview().as_deref(), Some(&b"applied"[..]));
}

#[test]
fn forget_drops_snapshot() {
    let tmp = tempfile::tempdir().unwrap();
    let st = state_with_bridge(tmp.path(), "applied");
    st.theme().arm_bridge_preview(b"applied".to_vec());
    std::fs::write(tmp.path().join("colors.json"), "freshly-applied").unwrap();

    forget(&st);

    assert!(!st.theme().bridge_preview_armed());
    assert_eq!(std::fs::read_to_string(tmp.path().join("colors.json")).unwrap(), "freshly-applied");
}

#[test]
fn early_end_restores() {
    let tmp = tempfile::tempdir().unwrap();
    let st = state_with_bridge(tmp.path(), "applied");

    arm(&st);
    std::fs::write(tmp.path().join("colors.json"), "hovered").unwrap();
    preview_end(&st);

    assert_eq!(std::fs::read_to_string(tmp.path().join("colors.json")).unwrap(), "applied");
}

#[test]
fn end_without_preview_noop() {
    let tmp = tempfile::tempdir().unwrap();
    let st = state_with_bridge(tmp.path(), "applied");
    preview_end(&st);
    assert_eq!(std::fs::read_to_string(tmp.path().join("colors.json")).unwrap(), "applied");
}
