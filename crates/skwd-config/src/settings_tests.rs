use super::*;

#[test]
fn we_renderer_canonicalizes() {
    for value in [
        serde_json::json!({"weRender": {"engine": "auto"}}),
        serde_json::json!({"weRender": {"engine": "compatibility"}}),
        serde_json::json!({"weRender": {"native": false}}),
        serde_json::json!({"weRender": {"native": true, "engine": "auto"}}),
    ] {
        let mut value = value;
        assert!(canonicalize_we_renderer(&mut value));
        assert_eq!(value["weRender"]["engine"], "native");
        assert!(value["weRender"].get("native").is_none());
        assert!(!canonicalize_we_renderer(&mut value));
    }
}
use serde_json::json;

#[test]
fn wall_directory_fallbacks() {
    let value = json!({"paths": {"wallpaper": "~/walls", "videoWallpaper": ""}});
    assert_eq!(wallpaper_dir(&value), format!("{}/walls", crate::home()));
    assert_eq!(video_dir(&value), wallpaper_dir(&value));
    assert_eq!(wallpaper_dir(&json!({})), format!("{}/Pictures/Wallpapers", crate::home()));
    assert_eq!(cache_dir_of(&json!({"paths": {"cache": "/x/c"}})), "/x/c");
}

#[test]
fn paper_engine_allowlist() {
    assert_eq!(paper_engine(&json!({})), "skwd-paper");
    assert_eq!(paper_engine(&json!({"paper": {"engine": "skwd-paper"}})), "skwd-paper");
    assert_eq!(paper_engine(&json!({"paper": {"engine": "awww"}})), "awww");
    for retired in ["noctalia", "dms", "swww", ""] {
        assert_eq!(paper_engine(&json!({"paper": {"engine": retired}})), "skwd-paper");
    }
    assert_eq!(paper_engine(&json!({"paper": {"engine": 7}})), "skwd-paper");
}

#[test]
fn retired_paper_engines_canonicalize() {
    for retired in [
        serde_json::json!({"paper": {"engine": "noctalia"}}),
        serde_json::json!({"paper": {"engine": "dms"}}),
        serde_json::json!({"paper": {"engine": 7}}),
    ] {
        let mut root = retired;
        assert!(canonicalize_paper_engine(&mut root));
        assert_eq!(root["paper"]["engine"], "skwd-paper");
        assert!(!canonicalize_paper_engine(&mut root));
    }
    let mut absent = serde_json::json!({});
    assert!(!canonicalize_paper_engine(&mut absent));
    let mut awww = serde_json::json!({"paper": {"engine": "awww"}});
    assert!(!canonicalize_paper_engine(&mut awww));
}

#[test]
fn audio_settings_bounded() {
    assert!(wallpaper_mute(&json!({})));
    assert!(!wallpaper_mute(&json!({"wallpaperMute": false})));
    assert_eq!(wallpaper_volume(&json!({})), 100);
    assert_eq!(wallpaper_volume(&json!({"wallpaperVolume": 250})), 100);
    assert_eq!(wallpaper_volume(&json!({"wallpaperVolume": 40})), 40);
}

#[test]
fn video_preview_defaults() {
    assert!(video_preview_enabled(&json!({})));
    assert!(!video_preview_enabled(&json!({"videoPreview": {"enabled": false}})));
    assert_eq!(video_preview_delay_ms(&json!({})), 250);
    assert_eq!(video_preview_delay_ms(&json!({"videoPreview": {"delayMs": 9000}})), 3000);
    assert!(crate::schema::find(crate::keys::video_preview::MODE).is_none());
}

#[test]
fn source_settings_defaults() {
    assert!(wallhaven_enabled(&json!({})));
    assert!(!steam_enabled(&json!({"features": {"steam": false}})));
    assert_eq!(unsplash_access_key(&json!({})), "");
    assert_eq!(pexels_api_key(&json!({"sources": {"pexels": {"apiKey": "k"}}})), "k");
    assert_eq!(locale(&json!({"general": {"locale": "sv"}})), "sv");
}

#[test]
fn theme_backend_default() {
    assert_eq!(theme_backend(&json!({})), "skwd-iris");
    assert_eq!(theme_backend(&json!({"features": {"matugen": false}})), "skwd-iris");
    assert_eq!(theme_backend(&json!({"theme": {"backend": "native"}})), "native");
}

#[test]
fn legacy_backend_splits() {
    let external = json!({"theme": {"backend": "noctalia"}});
    assert_eq!(theme_policy(&external), "wallpaper");
    assert_eq!(theme_authority(&external), "noctalia");
    assert_eq!(theme_engine(&external), "skwd-iris");
    assert_eq!(theme_backend(&external), "noctalia");

    let fixed = json!({"theme": {"backend": "static"}});
    assert_eq!(theme_policy(&fixed), "fixed");
    assert_eq!(theme_backend(&fixed), "static");
}

#[test]
fn explicit_theme_model_wins() {
    let local = json!({"theme": {
        "backend": "static",
        "policy": "wallpaper",
        "authority": "skwd",
        "engine": "wallust"
    }});
    assert_eq!(theme_policy(&local), "wallpaper");
    assert_eq!(theme_authority(&local), "skwd");
    assert_eq!(theme_engine(&local), "wallust");
    assert_eq!(theme_backend(&local), "wallust");

    let handed_off = json!({"theme": {
        "policy": "wallpaper",
        "authority": "dms",
        "engine": "pywal"
    }});
    assert_eq!(theme_engine(&handed_off), "pywal");
    assert_eq!(theme_backend(&handed_off), "dms");

    for authority in ["caelestia", "end4"] {
        let root = json!({"theme": {"policy": "wallpaper", "authority": authority}});
        assert_eq!(theme_authority(&root), authority);
        assert_eq!(theme_backend(&root), authority);
    }
}
