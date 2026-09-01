#![cfg(test)]

use super::*;

fn declarations() -> Map<String, Value> {
    serde_json::json!({
        "glow": {"type": "bool", "value": true, "text": "Glow", "order": 2},
        "tint": {"type": "color", "value": "1 1 1", "text": "Tint", "order": 1},
        "zoom": {"min": 0.5, "max": 3.0, "step": 0.01, "value": 1.0, "text": "Zoom", "order": 3},
        "mode": {
            "type": "combo",
            "value": 0,
            "text": "Mode",
            "order": 4,
            "options": [{"label": "Day", "value": 0}, {"label": "Night", "value": 1}]
        },
        "header": {"type": "group", "text": "Look", "order": 0},
        "keybind": {"type": "usershortcut", "value": "ctrl+g", "text": "Shortcut", "order": 5}
    })
    .as_object()
    .unwrap()
    .clone()
}

#[test]
fn typed_rows_authored_order() {
    let rows = merge(&declarations(), &Map::new());
    assert_eq!(
        rows.iter().map(|row| row.name.as_str()).collect::<Vec<_>>(),
        ["header", "tint", "glow", "zoom", "mode", "keybind"]
    );
    assert_eq!(
        rows.iter().map(|row| row.kind.as_str()).collect::<Vec<_>>(),
        [kind::GROUP, kind::COLOR, kind::BOOL, kind::SLIDER, kind::COMBO, kind::UNSUPPORTED]
    );
    assert!(rows.iter().all(|row| !row.overridden));
    assert_eq!(rows.iter().filter(|row| row.editable()).count(), 4);
}

#[test]
fn untyped_bounds_infer_slider() {
    let rows = merge(&declarations(), &Map::new());
    let zoom = rows.iter().find(|row| row.name == "zoom").unwrap();
    assert_eq!(zoom.kind, kind::SLIDER);
    assert_eq!(zoom.declared, "");
    assert_eq!((zoom.min, zoom.max, zoom.step), (Some(0.5), Some(3.0), Some(0.01)));
    assert_eq!(zoom.label, "Zoom");
}

#[test]
fn combo_options_overrides() {
    let overrides = serde_json::json!({"zoom": 2.5, "mode": 1}).as_object().unwrap().clone();
    let rows = merge(&declarations(), &overrides);
    let mode = rows.iter().find(|row| row.name == "mode").unwrap();
    assert_eq!(
        mode.options,
        vec![
            WePropertyOption { label: "Day".into(), value: 0.0 },
            WePropertyOption { label: "Night".into(), value: 1.0 },
        ]
    );
    assert_eq!(mode.value, serde_json::json!(1));
    assert_eq!(mode.default, serde_json::json!(0));
    assert!(mode.overridden);

    let zoom = rows.iter().find(|row| row.name == "zoom").unwrap();
    assert_eq!(zoom.value, serde_json::json!(2.5));
    assert_eq!(zoom.default, serde_json::json!(1.0));
    assert!(zoom.overridden);

    let tint = rows.iter().find(|row| row.name == "tint").unwrap();
    assert!(!tint.overridden);
    assert_eq!(tint.value, tint.default);
}

#[test]
fn unreadable_project_no_rows() {
    let dir = tempfile::tempdir().unwrap();
    assert!(read_declarations(dir.path()).is_empty());
    std::fs::write(dir.path().join("project.json"), b"{ not json").unwrap();
    assert!(read_declarations(dir.path()).is_empty());
    std::fs::write(
        dir.path().join("project.json"),
        b"\xEF\xBB\xBF{\"general\":{\"properties\":{\"a\":{\"type\":\"bool\",\"value\":false}}}}",
    )
    .unwrap();
    assert_eq!(read_declarations(dir.path()).len(), 1);
}
