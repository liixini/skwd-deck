#![cfg(test)]

use super::{parse_backdrop_source, pick_backdrop_source};

#[test]
fn follow_vs_fixed() {
    let last = r#"{"type":"static","path":"/w/desk.png","thumb":"/w/desk.png"}"#;
    assert_eq!(pick_backdrop_source(true, "/w/fixed.png", last).as_deref(), Some("/w/desk.png"));
    assert_eq!(pick_backdrop_source(false, "/w/fixed.png", last).as_deref(), Some("/w/fixed.png"));
    assert_eq!(pick_backdrop_source(false, "  ", last).as_deref(), Some("/w/desk.png"));
}

#[test]
fn preserves_media_source() {
    assert_eq!(
        parse_backdrop_source(r#"{"type":"video","path":"/w/clip.mp4","thumb":"/c/t.jpg"}"#)
            .as_deref(),
        Some("/w/clip.mp4"),
    );
    assert_eq!(
        parse_backdrop_source(r#"{"type":"static","path":"/w/a.png","thumb":""}"#).as_deref(),
        Some("/w/a.png"),
    );
    assert_eq!(parse_backdrop_source(r#"{"type":"we","path":"","thumb":""}"#), None);
    assert_eq!(parse_backdrop_source("not json"), None);
}

#[test]
fn follows_wallpaper_engine_source() {
    assert_eq!(parse_backdrop_source(r#"{"type":"we","we_id":"3260370312","path":"/we/video.mp4","thumb":"/cache/preview.png"}"#).as_deref(), Some("we:3260370312"));
}

#[test]
fn resolves_media_without_using_previews() {
    let root = tempfile::tempdir().unwrap();
    let config = skwd_wall_core::config::Config::from_root(serde_json::json!({}));
    for (name, kind) in [
        ("clip.mp4", skwd_wall_core::infrastructure::paper::SourceKind::Video),
        ("still.png", skwd_wall_core::infrastructure::paper::SourceKind::Static),
    ] {
        let path = root.path().join(name);
        std::fs::write(&path, []).unwrap();
        let source = super::media_source(&config, path.to_str().unwrap()).unwrap();
        assert_eq!(source.kind, kind);
        assert_eq!(source.path, path.to_str().unwrap());
    }
    assert_eq!(
        super::media_source(&config, root.path().to_str().unwrap()).unwrap().kind,
        skwd_wall_core::infrastructure::paper::SourceKind::WallpaperEngine
    );
    assert!(super::media_source(&config, "we:../escape").is_err());
    assert!(super::media_source(&config, "/missing/backdrop.mp4").is_err());
}

#[test]
fn legacy_cleanup_stays_in_its_wayland_session() {
    let environment = b"WAYLAND_DISPLAY=wayland-1\0XDG_RUNTIME_DIR=/run/user/1000\0";
    assert!(super::legacy_session_matches(environment, "wayland-1", "/run/user/1000"));
    assert!(!super::legacy_session_matches(environment, "wayland-2", "/run/user/1000"));
    assert!(!super::legacy_session_matches(environment, "wayland-1", "/tmp/private-runtime"));
}
