#![cfg(test)]

use super::{is_long_lived, is_swap_command};

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
