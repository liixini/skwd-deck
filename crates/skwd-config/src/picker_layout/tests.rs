use super::*;
use serde_json::json;

const VIEWPORT: Size = Size { width: 1280.0, height: 720.0 };

#[test]
fn profile_round_trip() {
    let mut profile = LayoutProfile::capture(&json!({}), VIEWPORT);
    profile.positions.insert(
        Component::Search,
        Position {
            reference: Reference::Picker,
            alignment: Alignment::Top,
            offset: Offset { x: 0.0, y: -32.0 },
        },
    );
    let value = serde_json::to_value(&profile).unwrap();
    assert_eq!(
        value["positions"]["search"],
        json!({"reference":"picker", "alignment":"top", "offset":{"x":0.0,"y":-32.0}})
    );
    assert_eq!(serde_json::from_value::<LayoutProfile>(value).unwrap(), profile);
}

#[test]
fn floating_positions_round_trip() {
    let mut profile = LayoutProfile::capture(&json!({}), VIEWPORT);
    let position = Position {
        reference: Reference::Screen,
        alignment: Alignment::BottomLeft,
        offset: Offset { x: 18.0, y: -18.0 },
    };
    profile.positions.insert(Component::Arrange, position);
    profile.positions.insert(Component::Swatch, position);
    let value = serde_json::to_value(&profile).unwrap();
    assert!(value["positions"]["arrange"].is_object());
    assert!(value["positions"]["swatch"].is_object());
    assert_eq!(serde_json::from_value::<LayoutProfile>(value).unwrap(), profile);
    assert!(profile.validate().is_ok());
}

const NEW_KEYS: [&str; 8] = [
    crate::keys::filter_bar::SHOW_ARRANGE,
    crate::keys::filter_bar::SHOW_SWATCH,
    crate::keys::filter_bar::ARRANGE_POSITION_ENABLED,
    crate::keys::filter_bar::ARRANGE_POSITION_X,
    crate::keys::filter_bar::ARRANGE_POSITION_Y,
    crate::keys::filter_bar::SWATCH_POSITION_ENABLED,
    crate::keys::filter_bar::SWATCH_POSITION_X,
    crate::keys::filter_bar::SWATCH_POSITION_Y,
];

#[test]
fn untouched_floats_omitted() {
    let document = json!({"filterBar": {"positionX": 4.0}});
    let profile = LayoutProfile::capture(&document, VIEWPORT);
    for key in NEW_KEYS {
        assert!(!profile.values.contains_key(key), "{key}");
    }
    let value = serde_json::to_value(&profile).unwrap();
    assert!(value["positions"].as_object().unwrap().is_empty());
    assert!(profile.values.contains_key(crate::keys::filter_bar::POSITION_X));
}

#[test]
fn set_floats_captured() {
    let document = json!({"filterBar": {"show": {"arrange": false}, "arrangePositionX": 12.0}});
    let profile = LayoutProfile::capture(&document, VIEWPORT);
    assert_eq!(profile.values[crate::keys::filter_bar::SHOW_ARRANGE], Some(json!(false)));
    assert_eq!(profile.values[crate::keys::filter_bar::ARRANGE_POSITION_X], Some(json!(12.0)));
    assert!(!profile.values.contains_key(crate::keys::filter_bar::SWATCH_POSITION_X));
}

#[test]
fn absent_floats_reset() {
    let moved = json!({"filterBar": {"arrangePositionEnabled": true, "arrangePositionX": 12.0}});
    let mut document = moved.clone();
    LayoutProfile::capture(&json!({}), VIEWPORT).apply(&mut document);
    assert!(crate::get(&document, crate::keys::filter_bar::ARRANGE_POSITION_X).is_none());
    assert!(crate::get(&document, crate::keys::filter_bar::ARRANGE_POSITION_ENABLED).is_none());
    LayoutProfile::capture(&moved, VIEWPORT).apply(&mut document);
    assert_eq!(
        crate::get(&document, crate::keys::filter_bar::ARRANGE_POSITION_X),
        Some(&json!(12.0))
    );
}

#[test]
fn hidden_float_stays_hidden() {
    let hidden = json!({"filterBar": {"show": {"arrange": false}, "arrangePositionX": 12.0}});
    let mut document = hidden;
    LayoutProfile::capture(&json!({}), VIEWPORT).apply(&mut document);
    assert_eq!(crate::get(&document, crate::keys::filter_bar::SHOW_ARRANGE), Some(&json!(false)));
    assert!(crate::get(&document, crate::keys::filter_bar::ARRANGE_POSITION_X).is_none());
}

#[test]
fn clearing_drops_float_key() {
    let mut profile = LayoutProfile::capture(&json!({}), VIEWPORT);
    profile.set(crate::keys::filter_bar::SHOW_SWATCH, Some(json!(false)));
    assert!(profile.values.contains_key(crate::keys::filter_bar::SHOW_SWATCH));
    profile.set(crate::keys::filter_bar::SHOW_SWATCH, None);
    assert!(!profile.values.contains_key(crate::keys::filter_bar::SHOW_SWATCH));
    profile.set(crate::keys::filter_bar::POSITION_X, None);
    assert_eq!(profile.values[crate::keys::filter_bar::POSITION_X], None);
}

#[test]
fn old_profile_still_reads() {
    let old = json!({"pickerLayouts":{"version":1,"outputs":{"DP-1":{
        "viewport":{"width":1280.0,"height":720.0},
        "values":{"filterBar.positionX":10.0},
        "positions":{}
    }}}});
    let profile = &Layouts::read(&old).unwrap().outputs["DP-1"];
    assert!(profile.positions.is_empty());
    assert!(!profile.values.contains_key(crate::keys::filter_bar::SHOW_ARRANGE));
}

#[test]
fn layout_excludes_behavior() {
    for key in [
        crate::keys::selector::LAST_APPLIED_KEY,
        crate::keys::selector::START_POSITION,
        crate::keys::filter_bar::LAST_KIND,
        crate::keys::filter_bar::LAST_SORT,
        crate::keys::semantic::ENABLED,
        crate::keys::motion::REDUCED,
    ] {
        assert!(!owns(key), "{key}");
    }
    for key in [
        crate::keys::selector::FRAME_X,
        crate::keys::selector::GRID_COLUMNS,
        crate::keys::selector::GRID_CORNER_RADIUS,
        crate::keys::filter_bar::BUTTON_ORDER,
        crate::keys::filter_bar::HIDDEN_BUTTONS,
        crate::keys::filter_bar::SHOW_ARRANGE,
        crate::keys::filter_bar::SHOW_SWATCH,
        crate::keys::filter_bar::ARRANGE_POSITION_ENABLED,
        crate::keys::filter_bar::ARRANGE_POSITION_X,
        crate::keys::filter_bar::ARRANGE_POSITION_Y,
        crate::keys::filter_bar::SWATCH_POSITION_ENABLED,
        crate::keys::filter_bar::SWATCH_POSITION_X,
        crate::keys::filter_bar::SWATCH_POSITION_Y,
    ] {
        assert!(owns(key), "{key}");
    }
}

#[test]
fn default_capture_clears_override() {
    let profile = LayoutProfile::capture(&json!({}), VIEWPORT);
    let mut document = json!({"components":{"wallpaperSelector":{"frameX":50}},"unrelated":true});
    profile.apply(&mut document);
    assert!(crate::get(&document, crate::keys::selector::FRAME_X).is_none());
    assert_eq!(document["unrelated"], true);
}

#[test]
fn relationships_reject_cycles() {
    let mut profile = LayoutProfile::capture(&json!({}), VIEWPORT);
    profile.positions.insert(
        Component::Picker,
        Position {
            reference: Reference::Picker,
            alignment: Alignment::Center,
            offset: Offset { x: 0.0, y: 0.0 },
        },
    );
    assert!(profile.validate().is_err());
    profile.positions.remove(&Component::Picker);
    profile.viewport.width = f32::NAN;
    assert!(profile.validate().is_err());
}

#[test]
fn layouts_reject_unknown_version() {
    assert!(Layouts::read(&json!({"pickerLayouts":{"version":2,"outputs":{}}})).is_err());
}

#[test]
fn reduced_motion_defaults_off() {
    assert_eq!(crate::schema::boolean_default(crate::keys::motion::REDUCED), Some(false));
    assert!(crate::schema::setting::motion::REDUCED.read(&json!({"motion":{"reduced":true}})));
}

#[test]
fn corners_round_trip() {
    let cut = json!({"components": {"wallpaperSelector": {
        "gridCornerStyle": "chamfered", "gridCornerTL": 9, "gridCornerAll": 14,
        "gridCornersSame": false
    }}});
    let profile = LayoutProfile::capture(&cut, VIEWPORT);
    let mut document = json!({});
    profile.apply(&mut document);
    assert_eq!(crate::corners::GRID.read(&document), crate::corners::GRID.read(&cut));
    LayoutProfile::capture(&json!({}), VIEWPORT).apply(&mut document);
    for path in [crate::corners::GRID.all].into_iter().chain(crate::corners::GRID.values) {
        assert!(crate::get(&document, path).is_none(), "{path}");
    }
    assert!(crate::get(&document, crate::keys::selector::GRID_CORNER_STYLE).is_none());
}

#[test]
fn legacy_profile_drops_corners() {
    let mut document = json!({"components": {"wallpaperSelector": {"handCornerStyle": "round"}}});
    let old = json!({"pickerLayouts":{"version":1,"outputs":{"DP-1":{
        "viewport":{"width":1280.0,"height":720.0},
        "values":{"components.wallpaperSelector.handCornerRadius":12.0},
        "positions":{}
    }}}});
    Layouts::read(&old).unwrap().outputs["DP-1"].apply(&mut document);
    let hand = crate::corners::HAND.read(&document);
    assert_eq!((hand.style, hand.sizes()), (crate::corners::CornerStyle::Round, [12.0; 4]));
}

#[test]
fn retired_bar_flag_reads() {
    for (old, mode) in [
        (json!(false), crate::FilterBarVisibility::Hover),
        (Value::Null, crate::FilterBarVisibility::Always),
    ] {
        let layouts = json!({"pickerLayouts":{"version":1,"outputs":{"DP-1":{
            "viewport":{"width":1280.0,"height":720.0},
            "values":{"general.filterBarAlwaysVisible":old},
            "positions":{}
        }}}});
        let mut document = json!({"general": {"filterBarVisibility": "hidden"}});
        Layouts::read(&layouts).unwrap().outputs["DP-1"].apply(&mut document);
        assert_eq!(crate::filter_bar_visibility(&document), mode);
        let legacy = crate::keys::general::LEGACY_FILTER_BAR_ALWAYS_VISIBLE;
        assert!(crate::get(&document, legacy).is_none());
    }
}
