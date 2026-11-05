#![cfg(test)]

use super::*;

#[test]
fn keys_unique() {
    let mut seen: Vec<&str> = keys().collect();
    let count = seen.len();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), count);
    for key in keys() {
        let source = spec(key).unwrap();
        assert_eq!(source.key, key);
        assert_eq!(source.provider.key(), key);
        assert_eq!(Provider::from_key(key), Some(source.provider));
    }
    assert!(Provider::from_key("hackernews").is_none());
    assert!(spec("hackernews").is_none());
    assert!(spec("").is_none());
}

#[test]
fn list_result_generation_is_additive() {
    let legacy = ListResult {
        results: vec![ListItem { id: String::from("one"), ..ListItem::default() }],
        ..ListResult::default()
    };
    let legacy = serde_json::to_value(&legacy).unwrap();
    assert!(legacy.get("generation").is_none());
    assert_eq!(legacy["current_page"], 1);

    let generated = ListResult { generation: Some(42), ..ListResult::default() };
    let generated = serde_json::to_value(&generated).unwrap();
    assert_eq!(generated["generation"], 42);
    assert_eq!(serde_json::from_value::<ListResult>(generated).unwrap().generation, Some(42));
}

#[test]
fn workshop_details_round_trip() {
    let wire = serde_json::json!({
        "id": "123",
        "workshop": {
            "description": "Sea at dusk", "creator_id": "7656119", "creator_name": "Ada",
            "subscriptions": 12, "votes_up": 9, "votes_down": 1,
            "time_created": 100, "time_updated": 200,
            "previews": [
                {"kind": "image", "url": "https://images.steamusercontent.com/a"},
                {"kind": "youtube", "id": "Abc123"}
            ]
        }
    });
    let item: ListItem = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(item.workshop.as_ref().unwrap().previews.len(), 2);
    let encoded = serde_json::to_value(item).unwrap();
    assert_eq!(encoded["workshop"], wire["workshop"]);
    assert!(serde_json::to_value(ListItem::default()).unwrap().get("workshop").is_none());
}

#[test]
fn workshop_gallery_round_trip() {
    let gallery = WorkshopGallery {
        id: "123".into(),
        source: "steam".into(),
        creator_name: "Ada".into(),
        previews: vec![WorkshopPreview::Youtube { id: "PEZIDtt1ceA".into() }],
    };
    let wire = serde_json::to_value(&gallery).unwrap();
    assert_eq!(wire["creator_name"], "Ada");
    assert_eq!(wire["previews"][0]["kind"], "youtube");
    assert_eq!(serde_json::from_value::<WorkshopGallery>(wire).unwrap(), gallery);
}

#[test]
fn preview_refresh_defaults_off() {
    let request: PreviewRequest = serde_json::from_value(serde_json::json!({
        "source": "steam", "id": "123", "full_url": "https://images.steamusercontent.com/a"
    }))
    .unwrap();
    assert!(!request.refresh);
    let refreshed = PreviewRequest { refresh: true, ..request };
    assert_eq!(serde_json::to_value(refreshed).unwrap()["refresh"], true);
}

#[test]
fn media_apply_kind() {
    assert_eq!(Media::Image.apply_kind(), "static");
    assert_eq!(Media::Video.apply_kind(), "video");
    assert_eq!(Media::Scene.apply_kind(), "we");
    assert_eq!(spec("youtube").unwrap().media, Media::Video);
    for src in SOURCES.iter().filter(|src| src.key != "youtube" && src.key != "steam") {
        assert_eq!(src.media, Media::Image);
    }
}

#[test]
fn native_rpc_only() {
    let native: Vec<&str> = SOURCES
        .iter()
        .filter(|src| src.transport == Transport::Native)
        .map(|src| src.key)
        .collect();
    assert_eq!(native, ["wallhaven", "steam"]);
}

#[test]
fn daily_not_searchable() {
    let fixed: Vec<&str> =
        SOURCES.iter().filter(|src| !src.searchable).map(|src| src.key).collect();
    assert_eq!(fixed, ["bing"]);
}
