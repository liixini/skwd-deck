use super::*;

#[test]
fn editable_kinds() {
    let editable =
        |kind: &str| WeProperty { kind: kind.into(), ..WeProperty::default() }.editable();
    assert!(editable(we_property_kind::BOOL));
    assert!(editable(we_property_kind::COLOR));
    assert!(editable(we_property_kind::SLIDER));
    assert!(editable(we_property_kind::COMBO));
    assert!(!editable(we_property_kind::GROUP));
    assert!(!editable(we_property_kind::UNSUPPORTED));
    assert!(!editable(""));
}

#[test]
fn row_round_trip() {
    let row = WeProperty {
        name: "tint".into(),
        label: "Tint".into(),
        kind: we_property_kind::COLOR.into(),
        declared: "color".into(),
        value: serde_json::json!("1 0 0"),
        default: serde_json::json!("1 1 1"),
        overridden: true,
        order: 12,
        ..WeProperty::default()
    };
    let encoded = serde_json::to_string(&row).unwrap();
    assert_eq!(serde_json::from_str::<WeProperty>(&encoded).unwrap(), row);
}
