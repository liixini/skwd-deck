use super::*;

#[test]
fn last_source_static_video() {
    assert_eq!(
        parse_last_source(r#"{"type":"video","path":"/w/clip.mp4"}"#),
        Some(("video".into(), "/w/clip.mp4".into())),
    );
    assert_eq!(
        parse_last_source(r#"{"type":"static","path":"/w/pic.png"}"#),
        Some(("static".into(), "/w/pic.png".into())),
    );
}

#[test]
fn last_source_invalid() {
    assert_eq!(parse_last_source(r#"{"type":"we","path":""}"#), None);
    assert_eq!(parse_last_source(r#"{"type":"static","path":""}"#), None);
    assert_eq!(parse_last_source(r#"{"type":"video"}"#), None);
    assert_eq!(parse_last_source("not json"), None);
}

#[test]
fn last_matches_exact() {
    let current = r#"{"type":"video","path":"/w/clip.mp4","we_id":"","mute":false,"volume":40,"thumb":"/w/t.webp"}"#;
    assert!(last_matches_json(current, "video", "/w/clip.mp4", "", false, 40));
    assert!(!last_matches_json(current, "video", "/w/other.mp4", "", false, 40));
    assert!(!last_matches_json(current, "static", "/w/clip.mp4", "", false, 40));
    assert!(!last_matches_json(current, "video", "/w/clip.mp4", "", true, 40));
    assert!(!last_matches_json(current, "video", "/w/clip.mp4", "", false, 80));
}

#[test]
fn last_matches_we() {
    let current = r#"{"type":"we","path":"","we_id":"12345","mute":true,"volume":0}"#;
    assert!(last_matches_json(current, "we", "", "12345", true, 0));
    assert!(!last_matches_json(current, "we", "", "99999", true, 0));
    assert!(!last_matches_json("not json", "video", "/w/clip.mp4", "", false, 40));
}
