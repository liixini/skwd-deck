use super::*;

fn tags(raw: &str) -> Vec<String> {
    crate::playlist::tag_tokens(raw)
}

fn item<'a>(
    key: &'a str,
    tags: &'a [String],
    kind: &'a str,
    hue: i64,
    width: i64,
    height: i64,
) -> Item<'a> {
    Item { key, tags, kind, hue, width, height }
}

#[test]
fn folder_from_key() {
    assert_eq!(folder_of("static:abstract/x.png"), "abstract");
    assert_eq!(folder_of("video:a/b/c.mp4"), "a/b");
    assert_eq!(folder_of("static:top.png"), "");
    assert_eq!(folder_of("we:123"), "");
}

#[test]
fn tag_spec_operators() {
    let cat_beach = tags("cat, beach, sunny");
    let cat_anime = tags("cat, anime, girl");
    let dog = tags("dog, park");
    assert!(matches_tag_spec(&cat_beach, "cat,beach"));
    assert!(!matches_tag_spec(&cat_beach, "cat,forest"));
    assert!(matches_tag_spec(&dog, "cat|dog"));
    assert!(matches_tag_spec(&cat_beach, "cat|dog,beach"));
    assert!(matches_tag_spec(&cat_beach, "cat,-anime"));
    assert!(!matches_tag_spec(&cat_anime, "cat,-anime"));
    assert!(!matches_tag_spec(&dog, "-anime"));
    assert!(matches_tag_spec(&cat_beach, " CAT , -ANIME "));
}

#[test]
fn dimension_and_ratio_filters() {
    let no_tags = Vec::new();
    let wide = item("static:x", &no_tags, "static", 99, 3840, 2160);
    assert!(matches(&wide, "width:>=1920 height:>1080"));
    assert!(matches(&wide, "res:>=1920x1080 ratio:landscape"));
    assert!(!matches(&wide, "res:>=3840x2161"));
    assert!(!matches(&wide, "ratio:portrait"));

    let flat = item("static:x", &no_tags, "static", 99, 100, 0);
    assert!(!matches(&flat, "ratio:landscape"));
}

#[test]
fn type_color_folder_tag() {
    let blue_tags = tags("cat,ocean");
    let video = item("video:nature/v.mp4", &blue_tags, "video", 8, 1920, 1080);
    assert!(matches(&video, "type:vid color:blue tag:cat folder:nature"));
    assert!(matches(&video, "type:video color:red,blue"));
    assert!(!matches(&video, "type:image"));
    assert!(!matches(&video, "tag:forest"));
    assert!(!matches(&video, ""));
    assert!(matches(&video, "all"));
    assert!(source_wants_favourites("favourites type:video"));
    assert!(!source_wants_favourites("type:video tag:cat"));
}
