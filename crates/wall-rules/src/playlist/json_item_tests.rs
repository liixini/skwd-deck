use super::*;

#[test]
fn tags_text_and_json() {
    assert_eq!(tag_tokens("Cat, beach"), vec!["cat", "beach"]);
    assert_eq!(tag_tokens(r#"["Cat","Dog",3]"#), vec!["cat", "dog"]);
}

#[test]
fn json_item_matching() {
    let video = serde_json::json!({
        "key": "video:nature/v.mp4",
        "tags": r#"["Cat","Ocean"]"#,
        "type": "video",
        "hue": 8,
        "width": 3840,
        "height": 2160
    });
    assert!(matches_item(&video, "type:video color:blue tag:cat res:>=1920x1080"));
    assert!(!matches_item(&video, "type:image"));
    assert!(!matches_item(&serde_json::json!({}), ""));
}
