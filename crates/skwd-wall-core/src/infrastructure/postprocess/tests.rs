#![cfg(test)]

use super::*;
use serde_json::json;

#[test]
fn substitute_quotes_tokens() {
    let out = substitute(
        "t=%type% n=%name% p=%path% th=%thumb%",
        "video",
        "clip.mp4",
        "/w/clip.mp4",
        "/c/t.png",
    );
    assert_eq!(out, "t='video' n='clip.mp4' p='/w/clip.mp4' th='/c/t.png'");
}

#[test]
fn substitute_blocks_injection() {
    let evil = "/w/$(touch /tmp/pwned).jpg";
    let out = substitute("cp %path% /backup/", "static", "$(touch /tmp/pwned).jpg", evil, "");
    assert!(out.contains("'/w/$(touch /tmp/pwned).jpg'"));
    assert!(!out.contains("cp $(touch"));
    let quoted = substitute("x %name%", "static", "a'b; rm -rf ~ #.jpg", "/w/a", "/c/t");
    assert_eq!(quoted, "x 'a'\\''b; rm -rf ~ #.jpg'");
}

#[test]
fn basename_final() {
    assert_eq!(basename("/a/b/wall.png"), "wall.png");
    assert_eq!(basename("wall.png"), "wall.png");
}

#[test]
fn postproc_entries() {
    let cfg = Config::from_root(json!({"postProcessing": [
        {"command": "a %path%"},
        {"command": "b", "type": "video"},
        {"command": "   "},
        "plain %name%"
    ]}));
    let got = cfg.post_processing();
    assert_eq!(got.len(), 3);
    assert_eq!(got[0], ("a %path%".to_string(), "all".to_string()));
    assert_eq!(got[1], ("b".to_string(), "video".to_string()));
    assert_eq!(got[2], ("plain %name%".to_string(), "all".to_string()));
}

#[test]
fn flags_default_off() {
    let defaults = Config::from_root(json!({}));
    assert!(!defaults.pick_only_mode());
    assert!(!defaults.post_process_on_restore());
    let cfg = Config::from_root(json!({"pickOnlyMode": true, "postProcessOnRestore": true}));
    assert!(cfg.pick_only_mode());
    assert!(cfg.post_process_on_restore());
}
