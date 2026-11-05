use serde_json::json;

use super::*;

#[test]
fn request_roundtrip() {
    assert_eq!(serde_json::to_value(StudioRequest::default()).unwrap(), json!({}));
    let request = StudioRequest {
        image: Some("/w/a.png".into()),
        area: Some(StudioArea { left: 0.25, top: 0.5, right: 0.75, bottom: 1.0 }),
        we_id: Some("42".into()),
        materials: true,
    };
    let value = serde_json::to_value(&request).unwrap();
    assert_eq!(
        value,
        json!({"image": "/w/a.png", "area": {"left": 0.25, "top": 0.5, "right": 0.75, "bottom": 1.0}, "we_id": "42", "materials": true})
    );
    assert_eq!(serde_json::from_value::<StudioRequest>(value).unwrap(), request);
}

#[test]
fn result_roundtrip() {
    let result = StudioResult {
        image: "/w/a.png".into(),
        seed: "#d77e52".into(),
        grey: false,
        wheel: vec![0.0, 1.0],
        methods: vec![StudioMethod {
            key: "tonal".into(),
            dark: json!({"base": "#000000"}),
            light: json!({"base": "#ffffff"}),
            mark: Some([0.5, 0.25]),
            mask: Vec::new(),
            sectors: vec![[20.0, 18.0]],
        }],
        materials: vec![StudioMethod {
            key: "smart".into(),
            dark: json!({"base": "#101010"}),
            light: json!({"base": "#fafafa"}),
            mark: None,
            mask: vec![[25.0, 1.0]],
            sectors: Vec::new(),
        }],
        dark: Some(false),
    };
    let value = serde_json::to_value(&result).unwrap();
    assert_eq!(value["methods"][0]["mark"], json!([0.5, 0.25]));
    assert_eq!(value["seed"], "#d77e52");
    assert_eq!(value["dark"], false);
    assert_eq!(value["methods"][0]["sectors"], json!([[20.0, 18.0]]));
    assert!(value["methods"][0].get("mask").is_none());
    assert_eq!(value["materials"][0]["key"], "smart");
    assert_eq!(serde_json::from_value::<StudioResult>(value).unwrap(), result);
    let bare = json!({"key": "subject", "dark": {}, "light": {}});
    assert_eq!(serde_json::from_value::<StudioMethod>(bare).unwrap().mark, None);
}

#[test]
fn area_needs_ordered_edges() {
    let area = StudioArea::from_edges(&[0.1, 0.2, 0.6, 0.9]).expect("area");
    assert_eq!((area.left, area.bottom), (0.1, 0.9));
    assert!(StudioArea::from_edges(&[0.6, 0.2, 0.1, 0.9]).is_none());
    assert!(StudioArea::from_edges(&[0.1, 0.2]).is_none());
}
