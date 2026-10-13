use serde_json::{Value, json};

use super::{AppThemeRefresh, AppThemeSet};

#[test]
fn app_set_roundtrip() {
    let request = AppThemeSet { id: "kitty".into(), enabled: true, adopt: false };
    let value = serde_json::to_value(&request).unwrap();
    assert_eq!(value, json!({"id": "kitty", "enabled": true, "adopt": false}));
    assert_eq!(serde_json::from_value::<AppThemeSet>(value).unwrap(), request);
    assert_eq!(
        serde_json::from_value::<AppThemeSet>(json!({"id": "kitty", "enabled": true})).unwrap(),
        request
    );
    assert!(serde_json::from_value::<AppThemeSet>(Value::Null).is_err());
}

#[test]
fn refresh_request_roundtrip() {
    let integration = AppThemeRefresh { integration: Some(2), profile: None };
    assert_eq!(serde_json::to_value(&integration).unwrap(), json!({"integration": 2}));
    assert_eq!(
        serde_json::from_value::<AppThemeRefresh>(json!({"integration": 2})).unwrap(),
        integration
    );
    let profile = AppThemeRefresh { integration: None, profile: Some("Nord".into()) };
    assert_eq!(serde_json::to_value(&profile).unwrap(), json!({"profile": "Nord"}));
    assert_eq!(
        serde_json::from_value::<AppThemeRefresh>(json!({"profile": "Nord"})).unwrap(),
        profile
    );
}
