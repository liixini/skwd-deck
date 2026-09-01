#![cfg(test)]

use super::*;
use crate::db::{list_wallpapers, open_in_memory, seed, update_user_tags};

#[test]
fn stem_key_strips() {
    assert_eq!(stem_key("static:effects/beach-cyberpunk.png"), "effects/beach-cyberpunk");
    assert_eq!(stem_key("static:effects/beach-cyberpunk.webp"), "effects/beach-cyberpunk");
    assert_eq!(stem_key("static:a.b/c.png"), "a.b/c");
    assert_eq!(stem_key("we:12345"), "12345");
    assert_eq!(stem_key("static:noext"), "noext");
}

#[test]
fn merge_tag_dedups() {
    assert_eq!(merge_tag("forest,sky", "cyberpunk"), "cyberpunk,forest,sky");
    assert_eq!(merge_tag("cyberpunk,forest", "cyberpunk"), "cyberpunk,forest");
    assert_eq!(merge_tag("Forest,CYBERPUNK", "cyberpunk"), "Forest,CYBERPUNK");
    assert_eq!(merge_tag("", "cyberpunk"), "cyberpunk");
    assert_eq!(merge_tag(r#"["forest","sky"]"#, "cyberpunk"), r#"["cyberpunk","forest","sky"]"#);
}

#[test]
fn effect_tag_survives() {
    let conn = open_in_memory().unwrap();
    set_effect_tag(&conn, "effects/beach-cyberpunk", "cyberpunk").unwrap();
    seed(&conn, "static:effects/beach-cyberpunk.webp", "effects/beach-cyberpunk.webp", "static");
    let list = list_wallpapers(&conn, false).unwrap();
    assert_eq!(list[0]["tags"], "cyberpunk");
    update_user_tags(&conn, "static:effects/beach-cyberpunk.webp", "forest,sky").unwrap();
    let after = list_wallpapers(&conn, false).unwrap();
    assert_eq!(after[0]["tags"], "cyberpunk,forest,sky");
}

#[test]
fn parse_effect_stems() {
    let ids: std::collections::HashSet<String> =
        ["grayscale", "sepia", "glitch"].iter().map(std::string::ToString::to_string).collect();
    assert_eq!(parse_effect_tag("beach-grayscale", &ids).as_deref(), Some("grayscale"));
    assert_eq!(parse_effect_tag("my-cool-beach-sepia", &ids).as_deref(), Some("sepia"));
    assert_eq!(parse_effect_tag("sunset-theme-rose-pine", &ids).as_deref(), Some("rose-pine"));
    assert_eq!(parse_effect_tag("my-theme-park-glitch", &ids).as_deref(), Some("glitch"));
    assert_eq!(parse_effect_tag("plain-wallpaper", &ids), None);
    let ids2: std::collections::HashSet<String> =
        ["gradientmap", "glitch"].iter().map(std::string::ToString::to_string).collect();
    assert_eq!(
        parse_effect_tag("Minimal-gradientmap-catppuccin", &ids2).as_deref(),
        Some("gradientmap")
    );
    assert_eq!(
        parse_effect_tag("cool-glitch-theme-rose-pine", &ids2).as_deref(),
        Some("rose-pine")
    );
}

#[test]
fn backfill_effects_only() {
    let conn = open_in_memory().unwrap();
    let ids: std::collections::HashSet<String> =
        ["grayscale", "sepia"].iter().map(std::string::ToString::to_string).collect();
    seed(&conn, "static:effects/beach-grayscale.webp", "effects/beach-grayscale.webp", "static");
    seed(&conn, "static:sub/effects/city-sepia.png", "sub/effects/city-sepia.png", "static");
    seed(&conn, "static:normal.webp", "normal.webp", "static");
    seed(
        &conn,
        "static:effects/mystery-unknownfx.webp",
        "effects/mystery-unknownfx.webp",
        "static",
    );
    assert_eq!(backfill_effect_tags(&conn, &ids).unwrap(), 2);
    assert_eq!(effect_tag(&conn, "effects/beach-grayscale").as_deref(), Some("grayscale"));
    assert_eq!(effect_tag(&conn, "sub/effects/city-sepia").as_deref(), Some("sepia"));
    assert!(effect_tag(&conn, "normal").is_none());
    let list = list_wallpapers(&conn, false).unwrap();
    let beach =
        list.iter().find(|item| item["key"] == "static:effects/beach-grayscale.webp").unwrap();
    assert_eq!(beach["tags"], "grayscale");
}

#[test]
fn effect_tag_replace() {
    let conn = open_in_memory().unwrap();
    set_effect_tag(&conn, "effects/x-grayscale", "grayscale").unwrap();
    assert_eq!(effect_tag(&conn, "effects/x-grayscale").as_deref(), Some("grayscale"));
    set_effect_tag(&conn, "effects/x-grayscale", "sepia").unwrap();
    assert_eq!(effect_tag(&conn, "effects/x-grayscale").as_deref(), Some("sepia"));
    assert!(effect_tag(&conn, "effects/missing").is_none());
}
