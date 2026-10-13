use super::{WallRemoveParams, WallRemoveResult};

#[test]
fn removal_wire_roundtrip() {
    let legacy = serde_json::json!({"we_id":"42"});
    let params: WallRemoveParams = serde_json::from_value(legacy.clone()).unwrap();
    assert_eq!(params.unsubscribe, None);
    assert_eq!(serde_json::to_value(params).unwrap(), legacy);

    let current = WallRemoveParams {
        we_id: Some("42".into()),
        unsubscribe: Some(true),
        ..WallRemoveParams::default()
    };
    assert_eq!(
        serde_json::to_value(current).unwrap(),
        serde_json::json!({"we_id":"42","unsubscribe":true})
    );
    let local = WallRemoveParams {
        we_id: Some("42".into()),
        unsubscribe: Some(false),
        ..WallRemoveParams::default()
    };
    assert_eq!(
        serde_json::to_value(local).unwrap(),
        serde_json::json!({"we_id":"42","unsubscribe":false})
    );

    let legacy_result: WallRemoveResult =
        serde_json::from_value(serde_json::json!({"removed":"we:42"})).unwrap();
    assert!(!legacy_result.unsubscribed);
}
