use serde_json::json;

use super::*;

fn selector(fields: Value) -> Value {
    json!({ "components": { "wallpaperSelector": fields } })
}

#[test]
fn defaults_keep_look() {
    let empty = json!({});
    let looks: [(CornerKeys, CornerStyle, f64); 5] = [
        (SLICES, CornerStyle::Straight, 16.0),
        (GRID, CornerStyle::Round, 6.0),
        (HAND, CornerStyle::Straight, 0.0),
        (DEPTH, CornerStyle::Straight, 0.0),
        (COLLECTION, CornerStyle::Round, 2.0),
    ];
    for (keys, style, value) in looks {
        let corners = keys.read(&empty);
        assert_eq!((corners.style, corners.same, corners.values), (style, true, [value; 4]));
    }
}

#[test]
fn legacy_fallback_per_mode() {
    let old = selector(json!({
        "roundCorners": true, "cornerRadius": 9, "cornerTL": 3,
        "gridRoundCorners": false, "gridCornerRadius": 18,
        "handCornerRadius": 14, "depthCorners": 7, "collectionCorners": 0
    }));
    let slices = SLICES.read(&old);
    assert_eq!(slices.style, CornerStyle::Round);
    assert_eq!(slices.values, [3.0, 9.0, 9.0, 9.0]);
    assert_eq!(slices.all, 3.0);
    assert!(!slices.same);
    assert_eq!(slices.sizes(), [3.0, 9.0, 9.0, 9.0]);
    let grid = GRID.read(&old);
    assert_eq!((grid.style, grid.values), (CornerStyle::Straight, [18.0; 4]));
    assert_eq!(grid.sizes(), [0.0; 4]);
    for (keys, style, value) in [
        (HAND, CornerStyle::Round, 14.0),
        (DEPTH, CornerStyle::Round, 7.0),
        (COLLECTION, CornerStyle::Straight, 0.0),
    ] {
        let corners = keys.read(&old);
        assert_eq!((corners.style, corners.same, corners.sizes()), (style, true, [value; 4]));
    }
}

#[test]
fn new_keys_win() {
    let doc = selector(json!({
        "gridRoundCorners": true, "gridCornerRadius": 18,
        "gridCornerStyle": "chamfered", "gridCornersSame": false,
        "gridCornerTL": 4, "gridCornerBR": 90
    }));
    let grid = GRID.read(&doc);
    assert_eq!(grid.style, CornerStyle::Chamfered);
    assert_eq!(grid.values, [4.0, 18.0, 64.0, 18.0]);
    assert_eq!(grid.sizes(), grid.values);
}

#[test]
fn same_uses_all() {
    let doc = selector(json!({
        "handCornerStyle": "round", "handCornersSame": true,
        "handCornerAll": 8, "handCornerTL": 5, "handCornerTR": 30
    }));
    assert_eq!(HAND.read(&doc).sizes(), [8.0; 4]);
    assert_eq!(HAND.read(&doc).shared(), 8.0);
    assert_eq!(number(&doc, key::HAND_CORNER_ALL), Some(8.0));
}

#[test]
fn same_off_keeps_values() {
    let doc = selector(json!({
        "handCornerStyle": "round", "handCornersSame": false,
        "handCornerAll": 8, "handCornerTL": 5, "handCornerTR": 30
    }));
    assert_eq!(HAND.read(&doc).sizes(), [5.0, 30.0, 0.0, 0.0]);
}

#[test]
fn stored_same_needs_all() {
    let doc = selector(json!({
        "handCornerStyle": "round", "handCornersSame": true,
        "handCornerTL": 5, "handCornerTR": 30
    }));
    let hand = HAND.read(&doc);
    assert!(!hand.same);
    assert_eq!(hand.all, 5.0);
    assert_eq!(hand.sizes(), [5.0, 30.0, 0.0, 0.0]);
}

#[test]
fn bad_style_falls_back() {
    let doc = selector(json!({ "depthCornerStyle": "wobbly", "depthCorners": 3 }));
    assert_eq!(DEPTH.read(&doc).style, CornerStyle::Round);
}

#[test]
fn paths_resolve() {
    let doc = selector(json!({ "collectionCornerStyle": "chamfered", "collectionCornerBL": 11 }));
    assert_eq!(number(&doc, key::COLLECTION_CORNER_BL), Some(11.0));
    assert_eq!(number(&doc, key::COLLECTION_CORNER_TL), Some(2.0));
    assert_eq!(style(&doc, key::COLLECTION_CORNER_STYLE), Some("chamfered"));
    assert_eq!(same(&doc, key::COLLECTION_CORNERS_SAME), Some(false));
    assert_eq!(number(&doc, key::GRID_CORNER_RADIUS), None);
    assert_eq!(owner(key::CORNER_TR), Some(&SLICES));
}

#[test]
fn style_keys_round_trip() {
    for style in CornerStyle::ALL {
        assert_eq!(CornerStyle::from_key(style.key()), Some(style));
    }
    assert_eq!(CornerStyle::ALL.map(CornerStyle::key), ["straight", "round", "chamfered"]);
}
