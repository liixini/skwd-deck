#![cfg(test)]

use super::{kill_args, parse_backdrop_source, pick_backdrop_source};

#[test]
fn follow_vs_fixed() {
    let last = r#"{"type":"static","path":"/w/desk.png","thumb":"/w/desk.png"}"#;
    assert_eq!(pick_backdrop_source(true, "/w/fixed.png", last).as_deref(), Some("/w/desk.png"));
    assert_eq!(pick_backdrop_source(false, "/w/fixed.png", last).as_deref(), Some("/w/fixed.png"));
    assert_eq!(pick_backdrop_source(false, "  ", last).as_deref(), Some("/w/desk.png"));
}

#[test]
fn thumb_then_path() {
    assert_eq!(
        parse_backdrop_source(r#"{"type":"video","path":"/w/clip.mp4","thumb":"/c/t.jpg"}"#)
            .as_deref(),
        Some("/c/t.jpg"),
    );
    assert_eq!(
        parse_backdrop_source(r#"{"type":"static","path":"/w/a.png","thumb":""}"#).as_deref(),
        Some("/w/a.png"),
    );
    assert_eq!(parse_backdrop_source(r#"{"type":"we","path":"","thumb":""}"#), None);
    assert_eq!(parse_backdrop_source("not json"), None);
}

#[test]
fn kill_args_separator() {
    let args = kill_args();
    assert_eq!(args[1], "--");
    assert!(args[2].starts_with("--namespace "));
}
