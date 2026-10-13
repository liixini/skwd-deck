use super::WallpaperItem;

#[test]
fn subscription_field_roundtrips() {
    let subscribed = WallpaperItem { steam_subscribed: true, ..WallpaperItem::default() };
    let value = serde_json::to_value(&subscribed).unwrap();
    assert_eq!(value["steam_subscribed"], true);
    assert!(serde_json::from_value::<WallpaperItem>(value).unwrap().steam_subscribed);

    let legacy = serde_json::json!({"key":"we:42"});
    assert!(!serde_json::from_value::<WallpaperItem>(legacy).unwrap().steam_subscribed);
    assert!(
        serde_json::to_value(WallpaperItem::default()).unwrap().get("steam_subscribed").is_none()
    );
}

#[test]
fn confirmation_field_roundtrips() {
    let blocked = WallpaperItem { steam_confirmation_required: true, ..WallpaperItem::default() };
    let value = serde_json::to_value(&blocked).unwrap();
    assert_eq!(value["steam_confirmation_required"], true);
    assert!(serde_json::from_value::<WallpaperItem>(value).unwrap().steam_confirmation_required);
    let legacy = serde_json::json!({"key":"we:42"});
    assert!(!serde_json::from_value::<WallpaperItem>(legacy).unwrap().steam_confirmation_required);
    assert!(
        serde_json::to_value(WallpaperItem::default())
            .unwrap()
            .get("steam_confirmation_required")
            .is_none()
    );
}
