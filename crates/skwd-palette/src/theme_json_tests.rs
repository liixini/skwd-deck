use crate::{Rgb, derive};

#[test]
fn legacy_and_canonical_roles() {
    let value = derive(&[Rgb(200, 100, 50)], true).to_value();
    for key in [
        "primary",
        "primaryText",
        "on_primary",
        "surface",
        "surfaceText",
        "on_surface",
        "surfaceVariant",
        "surfaceContainer",
        "background",
        "outline",
        "tertiary",
    ] {
        assert!(value.get(key).and_then(serde_json::Value::as_str).is_some(), "{key}");
    }
}
