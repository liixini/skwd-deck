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

#[test]
fn preview_cache_key_ignores_unrelated_profiles_and_saved_themes() {
    use serde_json::json;

    let base = json!({"theme": {"policy": "fixed", "staticTheme": "chosen", "savedThemes": [
        {"name": "chosen", "primary": "#123456"}
    ]}});
    let key = cache_key(&Config::from_root(base.clone()), "/image.png");
    let mut large = base;
    large["theme"]["wallpaperProfiles"] = json!(
        (0..78)
            .map(|index| json!({
                "key": format!("wallpaper-{index}"), "dark": {"_scheme": vec!["#123456"; 1500]}
            }))
            .collect::<Vec<_>>()
    );
    large["theme"]["savedThemes"].as_array_mut().unwrap().push(json!({
        "name": "unrelated", "_scheme": vec!["#abcdef"; 1500]
    }));
    assert_eq!(cache_key(&Config::from_root(large.clone()), "/image.png"), key);
    assert!(key.len() < 1024);
    large["theme"]["savedThemes"][0]["primary"] = json!("#abcdef");
    assert_ne!(cache_key(&Config::from_root(large.clone()), "/image.png"), key);
    assert_ne!(cache_key(&Config::from_root(large), "/other.png"), key);
}

#[test]
fn preview_cache_key_tracks_custom_colours_and_settings() {
    use serde_json::json;

    let base = Config::from_root(
        json!({"theme": {"policy": "fixed", "staticTheme": "custom", "customColors": ["#123456"]}}),
    );
    let key = cache_key(&base, "/image.png");
    for (path, value) in [
        ("theme.customColors", json!(["#abcdef"])),
        ("theme.mode", json!("light")),
        ("theme.staticTheme", json!("nord")),
        ("theme.style", json!("pastel")),
    ] {
        assert_ne!(cache_key(&base.with_override(path, value), "/image.png"), key, "{path}");
    }
}

#[test]
fn edited_profiles_override_cached_palettes_without_stale_results() {
    use serde_json::json;

    let state = WallState::test_new(json!({}));
    let base = Config::from_root(json!({"theme": {"policy": "wallpaper", "mode": "dark"}}));
    let cached = json!({"primary": "#987654"});
    state
        .theme()
        .cache_shell_palette(cache_key(&base, "/image.png"), cached.to_string().into_bytes());
    let palette = serde_json::Value::Object(
        crate::theme::profiles::ROLE_KEYS
            .into_iter()
            .map(|key| (key.into(), json!("#123456")))
            .collect(),
    );
    let mut profile = json!({"key": "/image.png", "enabled": true, "dark": palette});
    for colour in ["#123456", "#abcdef"] {
        profile["dark"]["primary"] = json!(colour);
        let config = base.with_override("theme.wallpaperProfiles", json!([profile]));
        assert_eq!(
            super::cached_palette_for_config(&state, &config, "/image.png").unwrap()["primary"],
            colour
        );
    }
    profile["enabled"] = json!(false);
    let config = base.with_override("theme.wallpaperProfiles", json!([profile]));
    assert_eq!(super::cached_palette_for_config(&state, &config, "/image.png"), Some(cached));
}
