use super::{preview_cache_id, require_workshop_preview};

#[test]
fn cache_tracks_preview_url() {
    let first = preview_cache_id("123", "https://images.steamusercontent.com/a");
    let second = preview_cache_id("123", "https://images.steamusercontent.com/b");
    assert_ne!(first, second);
    assert_eq!(first, preview_cache_id("123", "https://images.steamusercontent.com/a"));
}

#[test]
fn preview_host_filter() {
    assert!(require_workshop_preview("https://images.steamusercontent.com/ugc/a/b.jpg").is_ok());
    assert!(require_workshop_preview("https://i.ytimg.com/vi/PEZIDtt1ceA/hqdefault.jpg").is_ok());
    assert!(require_workshop_preview("https://www.youtube.com/watch?v=PEZIDtt1ceA").is_err());
    assert!(require_workshop_preview("https://attacker.example/preview.jpg").is_err());
}
