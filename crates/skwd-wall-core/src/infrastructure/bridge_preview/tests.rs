#![cfg(test)]

use super::{arm, cache_key, forget, preview_end};
use crate::config::Config;
use crate::state::WallState;
use std::sync::Barrier;
use std::sync::atomic::{AtomicUsize, Ordering};

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

    assert!(super::arm(&st, st.theme().shell_preview_generation()));
    std::fs::write(&kde, "HOVERED").unwrap();
    super::preview_end(&st, None);

    assert_eq!(std::fs::read_to_string(&kde).unwrap(), "APPLIED");
}

#[test]
fn end_restores_palette() {
    let tmp = tempfile::tempdir().unwrap();
    let st = state_with_bridge(tmp.path(), r##"{"background":"#applied"}"##);
    st.theme().arm_bridge_preview(std::fs::read(tmp.path().join("colors.json")).unwrap());
    std::fs::write(tmp.path().join("colors.json"), r##"{"background":"#hovered"}"##).unwrap();

    preview_end(&st, None);

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

    assert!(arm(&st, st.theme().shell_preview_generation()));
    std::fs::write(tmp.path().join("colors.json"), "hovered").unwrap();
    preview_end(&st, None);

    assert_eq!(std::fs::read_to_string(tmp.path().join("colors.json")).unwrap(), "applied");
}

#[test]
fn end_without_preview_noop() {
    let tmp = tempfile::tempdir().unwrap();
    let st = state_with_bridge(tmp.path(), "applied");
    preview_end(&st, None);
    assert_eq!(std::fs::read_to_string(tmp.path().join("colors.json")).unwrap(), "applied");
}

#[test]
fn stale_arm_preserves() {
    let tmp = tempfile::tempdir().unwrap();
    let st = state_with_bridge(tmp.path(), "applied");
    let stale = st.theme().shell_preview_generation();
    st.theme().bump_shell_preview();

    assert!(!arm(&st, stale));
    assert!(!st.theme().bridge_preview_armed());
    assert_eq!(std::fs::read_to_string(tmp.path().join("colors.json")).unwrap(), "applied");
}

#[test]
fn cancel_owns_generation() {
    let state = WallState::test_new(serde_json::json!({}));
    let old = state.theme().shell_preview_generation();
    let next = state.theme().cancel_shell_preview(old).unwrap();
    assert_eq!(next, state.theme().shell_preview_generation());
    assert_eq!(state.theme().cancel_shell_preview(old), None);
    assert_eq!(state.theme().shell_preview_generation(), next);
}

#[test]
fn blocked_end_preserves_hover() {
    let dir = tempfile::tempdir().unwrap();
    let state = state_with_bridge(dir.path(), "applied");
    let old = state.theme().shell_preview_generation();
    assert!(arm(&state, old));
    std::fs::write(dir.path().join("colors.json"), "old-hover").unwrap();
    let serial = state.theme().lock_shell_preview();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();

    std::thread::scope(|scope| {
        let old_end = scope.spawn(|| {
            started_tx.send(()).unwrap();
            preview_end(&state, Some(old));
            finished_tx.send(()).unwrap();
        });
        started_rx.recv().unwrap();
        assert!(finished_rx.try_recv().is_err());
        state.theme().bump_shell_preview();
        std::fs::write(dir.path().join("colors.json"), "new-hover").unwrap();
        drop(serial);
        old_end.join().unwrap();
    });

    assert_eq!(std::fs::read_to_string(dir.path().join("colors.json")).unwrap(), "new-hover");
    assert!(state.theme().bridge_preview_armed());
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

#[test]
fn concurrent_palette_generates_once() {
    const CALLERS: usize = 8;
    const GENERATION_DELAY: std::time::Duration = std::time::Duration::from_millis(20);
    let state = WallState::test_new(serde_json::json!({}));
    let start = Barrier::new(CALLERS);
    let calls = AtomicUsize::new(0);

    std::thread::scope(|scope| {
        let threads: Vec<_> = (0..CALLERS)
            .map(|_| {
                scope.spawn(|| {
                    start.wait();
                    state.theme().palette_or_generate("shared", || {
                        calls.fetch_add(1, Ordering::SeqCst);
                        std::thread::sleep(GENERATION_DELAY);
                        Some(b"palette".to_vec())
                    })
                })
            })
            .collect();
        for thread in threads {
            assert_eq!(thread.join().unwrap(), Some(b"palette".to_vec()));
        }
    });
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn failed_palette_retries() {
    let state = WallState::test_new(serde_json::json!({}));
    let calls = AtomicUsize::new(0);
    let first = state.theme().palette_or_generate("missing", || {
        calls.fetch_add(1, Ordering::SeqCst);
        None
    });
    let second = state.theme().palette_or_generate("missing", || {
        calls.fetch_add(1, Ordering::SeqCst);
        Some(b"ready".to_vec())
    });
    assert_eq!(first, None);
    assert_eq!(second, Some(b"ready".to_vec()));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn source_replacement_invalidates_palette() {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("image.png");
    let replacement = dir.path().join("replacement.png");
    let config = Config::from_root(serde_json::json!({}));
    let state = WallState::test_new(serde_json::json!({}));
    std::fs::write(&image, b"first").unwrap();
    let path = image.to_str().unwrap();
    let first_key = cache_key(&config, path);
    let first = state.theme().palette_or_generate(&first_key, || std::fs::read(&image).ok());
    std::fs::write(&replacement, b"second").unwrap();
    std::fs::rename(&replacement, &image).unwrap();
    let next_key = cache_key(&config, path);
    let next = state.theme().palette_or_generate(&next_key, || std::fs::read(&image).ok());
    assert_ne!(first_key, next_key);
    assert_eq!(first, Some(b"first".to_vec()));
    assert_eq!(next, Some(b"second".to_vec()));
}

#[test]
fn cached_roles_match() {
    let state = WallState::test_new(serde_json::json!({}));
    let config = Config::from_root(serde_json::json!({
        "theme": { "backend": "static", "staticTheme": "nord", "mode": "dark" }
    }));
    let image = "/unused-static-image.png";
    let expected = crate::theme::preview_palette(&config, image);
    assert_eq!(super::cached_palette_for_config(&state, &config, image), expected);
    assert_eq!(super::cached_palette_for_config(&state, &config, image), expected);
}
