#![cfg(test)]

use super::{is_long_lived, is_swap_command, refuse_capture};

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(ToString::to_string).collect()
}

#[test]
fn persist_flag_long_lived() {
    assert!(is_long_lived(&args(&["*", "/wall.png", "--persist"])));
}

#[test]
fn video_output_long_lived() {
    assert!(is_long_lived(&args(&[
        "*",
        "/video.mp4",
        "--fill-mode",
        "fill",
        "-o",
        "mute=yes;volume=100",
    ])));
}

#[test]
fn transition_overlay_short_lived() {
    assert!(!is_long_lived(&args(&[
        "*",
        "/to.png",
        "--transition-from",
        "/from.png",
        "--layer",
        "bottom",
    ])));
}

#[test]
fn swap_command_detection() {
    assert!(is_swap_command(r#"{"to":"/video/next.mp4","mute":false,"volume":60}"#));
    assert!(is_swap_command(r#"{"path":"/wall/next.png","fill":"fill"}"#));
    assert!(!is_swap_command(r#"{"mute":true,"volume":20}"#));
    assert!(!is_swap_command(r#"{"pause":true}"#));
    assert!(!is_swap_command("not json"));
}

#[test]
fn refuses_frame_captures() {
    let dir = std::env::temp_dir().join(format!("fake-renderer-capture-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let snapshot = dir.join("snapshot.ppm");
    let freeze = dir.join("freeze.ppm");
    refuse_capture(
        &serde_json::json!({"snapshot": {"path": snapshot, "output": "DP-1"}}).to_string(),
    );
    refuse_capture(&serde_json::json!({"pause": true, "freeze": freeze}).to_string());
    refuse_capture(r#"{"pause":true}"#);
    let errors = std::fs::read_dir(&dir).unwrap().count();
    let refused =
        dir.join("snapshot.ppm.error").is_file() && dir.join("freeze.ppm.error").is_file();
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(refused);
    assert_eq!(errors, 2);
}
