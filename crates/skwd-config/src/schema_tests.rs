use std::collections::HashSet;

use serde_json::json;

use super::*;

#[test]
fn typed_settings_defaults() {
    assert_eq!(read_text(&json!({}), crate::keys::noctalia::THEME_MODE).as_deref(), Some("follow"));
    assert_eq!(setting::general::MAX_FPS.read(&json!({})), 120.0);
    assert_eq!(setting::general::MAX_FPS.read(&json!({"general": {"maxFps": 90}})), 90.0);
    assert!(setting::general::FILTER_BAR_ALWAYS_VISIBLE.read(&json!({})));
    assert_eq!(setting::filter_bar::VISUAL_STYLE.read(&json!({})), "match");
    assert_eq!(setting::filter_bar::ORIENTATION.read(&json!({})), "horizontal");
    assert_eq!(setting::selector::SLICE_STAGE_X.read(&json!({})), 0.0);
    assert_eq!(setting::selector::SLICE_FILTER_BAR_OFFSET_X.read(&json!({})), 0.0);
    assert_eq!(setting::selector::HEX_SEARCH_PANEL_OFFSET_Y.read(&json!({})), 0.0);
    assert_eq!(
        setting::selector::GRID_FILTER_BAR_OFFSET_X
            .read(&json!({"components": {"wallpaperSelector": {"gridFilterBarOffsetX": 48}}})),
        48.0
    );
    assert_eq!(
        setting::selector::SANDY_SEARCH_PANEL_OFFSET_Y
            .read(&json!({"components": {"wallpaperSelector": {"sandySearchPanelOffsetY": -72}}})),
        -72.0
    );
    assert_eq!(
        setting::selector::SANDY_STAGE_Y
            .read(&json!({"components": {"wallpaperSelector": {"sandyStageY": 140}}})),
        100.0
    );
    assert_eq!(setting::general::SETTINGS_STYLE.read(&json!({})), "editorial");
    assert_eq!(setting::general::UI_SCALE.read(&json!({"general": {"uiScale": 9}})), 2.0);
    assert_eq!(
        read_number(&json!({"motion": {"fastMs": 2}}), crate::keys::motion::FAST_MS),
        Some(35.0)
    );
    assert_eq!(read_boolean(&json!({}), crate::keys::library::POLLING_FALLBACK), Some(false));
    assert_eq!(
        read_number(
            &json!({"library": {"pollingIntervalSeconds": 1}}),
            crate::keys::library::POLLING_INTERVAL_SECONDS,
        ),
        Some(15.0)
    );
}

#[test]
fn paths_unique_kinds_stable() {
    let mut paths = HashSet::new();
    for spec in all() {
        assert!(paths.insert(spec.path), "duplicate schema path: {}", spec.path);
    }
    assert_eq!(find(crate::keys::general::MAX_FPS).map(|spec| spec.kind), Some(ValueKind::Number));
    assert_eq!(
        find(crate::keys::filter_bar::VISUAL_STYLE).map(|spec| spec.kind),
        Some(ValueKind::Text)
    );
}

#[test]
fn dynamic_default_families() {
    assert_eq!(boolean_default("filterBar.show.folder"), Some(true));
    assert_eq!(boolean_default("components.wallpaperSelector.hexArc"), Some(true));
    assert_eq!(boolean_default(crate::keys::selector::SLICE_WOBBLE), Some(false));
    assert_eq!(boolean_default("display.outputLocks.DP-1"), Some(false));
    assert_eq!(value_kind("display.fillModes.DP-1"), Some(ValueKind::Text));
    assert_eq!(value_kind("transition.shaderScopes.sand-donut"), Some(ValueKind::Text));
    assert_eq!(
        text_default("components.wallpaperSelector.presets.demo.displayMode"),
        Some("slices")
    );
}

#[test]
fn normalizes_writes() {
    assert_eq!(normalize_value(crate::keys::general::UI_SCALE, &json!(9)), Some(json!(2)));
    assert_eq!(normalize_value(crate::keys::matugen::COLOR_INDEX, &json!("2")), Some(json!(2)));
    assert_eq!(
        normalize_value(crate::keys::schedule::LATITUDE, &json!(59.33)),
        Some(json!("59.33"))
    );
    assert_eq!(normalize_value(crate::keys::general::UI_SCALE, &json!("nope")), None);
    assert_eq!(value_kind("integrations.3.livePreview"), Some(ValueKind::Boolean));
    assert_eq!(value_kind("filterBar.resolutionPresets.2.width"), Some(ValueKind::Number));
    assert_eq!(value_kind("filterBar.resolutionPresets.2.from"), Some(ValueKind::Text));
    assert_eq!(value_kind("filterBar.resolutionPresets.2.to"), Some(ValueKind::Text));
    assert_eq!(value_kind("filterBar.resolutionPresets.2.orientation"), Some(ValueKind::Text));
}

#[test]
fn interface_language_is_independent_of_weather_location() {
    let language = crate::keys::general::LANGUAGE;
    assert_eq!(text_default(language), Some("auto"));
    assert_eq!(normalize_value(language, &json!("es-ES")), Some(json!("es-ES")));
    let data = json!({"general": {"language": "es-ES", "locale": "Stockholm"}});
    assert_eq!(crate::str_at(&data, language, "auto"), "es-ES");
    assert_eq!(crate::locale(&data), "Stockholm");
}

#[test]
fn browser_defaults_have_typed_values_and_keep_existing_search_defaults() {
    use crate::keys::{steam, wallhaven};
    assert_eq!(read_text(&json!({}), wallhaven::DEFAULT_SORT).as_deref(), Some("toplist"));
    assert_eq!(read_text(&json!({}), steam::DEFAULT_SORT).as_deref(), Some("3"));
    assert_eq!(read_boolean(&json!({}), wallhaven::DEFAULT_GENERAL), Some(true));
    assert_eq!(read_boolean(&json!({}), wallhaven::DEFAULT_NSFW), Some(false));
    assert_eq!(normalize_value(wallhaven::DEFAULT_ANIME, &json!(false)), Some(json!(false)));
    assert_eq!(normalize_value(wallhaven::DEFAULT_ANIME, &json!("false")), None);
    assert_eq!(normalize_value(steam::DEFAULT_TYPE, &json!("Scene")), Some(json!("Scene")));
}

#[test]
fn source_apply_button_is_optional_and_requires_a_boolean() {
    for path in [
        crate::keys::sources::WALLHAVEN_SHOW_APPLY_BUTTON,
        crate::keys::sources::STEAM_SHOW_APPLY_BUTTON,
        crate::keys::sources::UNSPLASH_SHOW_APPLY_BUTTON,
        crate::keys::sources::PEXELS_SHOW_APPLY_BUTTON,
        crate::keys::sources::YOUTUBE_SHOW_APPLY_BUTTON,
    ] {
        assert_eq!(read_boolean(&json!({}), path), Some(false));
        assert_eq!(normalize_value(path, &json!(true)), Some(json!(true)));
        assert_eq!(normalize_value(path, &json!("true")), None);
    }
    assert!(read_boolean(&json!({}), "sources.showApplyButton").is_none());
}

#[test]
fn folder_keybindings_and_hidden_visibility_are_typed() {
    use crate::keys::{filter_bar, keybind};
    assert_eq!(boolean_default(filter_bar::LAST_SHOW_HIDDEN_FOLDERS), Some(true));
    for key in [
        keybind::FOLDER_PREV,
        keybind::FOLDER_NEXT,
        keybind::FOLDER_TOGGLE,
        keybind::HIDDEN_FOLDERS,
    ] {
        assert_eq!(value_kind(key), Some(ValueKind::Text));
        assert_eq!(text_default(key), Some(""));
    }
}
