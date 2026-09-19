use super::*;

#[test]
fn snapshots_include_defaults_and_canonical_legacy_choices() {
    let snapshot =
        snapshot(&json!({"theme": {"backend": "matugen"}, "matugen": {"mode": "light"}}));
    assert_eq!(snapshot.len(), KEYS.len());
    assert_eq!(snapshot[keys::theme::MODE], "light");
    assert_eq!(snapshot[keys::theme::POLICY], "wallpaper");
    assert_eq!(snapshot[keys::theme::ENGINE], "matugen");
    assert_eq!(snapshot[keys::theme::STYLE], "natural");
    assert_eq!(snapshot[keys::matugen::COLOR_INDEX], 0);
    assert_eq!(snapshot[keys::theme::NOCTALIA_PURE_BLACK], false);
}

#[test]
fn pinned_settings_reject_unrelated_keys_and_invalid_types() {
    let profiles = [json!({"key": "static:a.png", "settingsPinned": true, "settings": {
        "theme.mode": "light", "theme.noctaliaPureBlack": {},
        "paths.wallpaper": "/different", "matugen.colorIndex": 99
    }})];
    let selected = settings(&profiles, "static:a.png").unwrap();
    assert_eq!(selected[keys::theme::MODE], "light");
    assert_eq!(selected[keys::matugen::COLOR_INDEX], 3);
    assert!(!selected.contains_key(keys::paths::WALLPAPER));
    assert!(!selected.contains_key(keys::theme::NOCTALIA_PURE_BLACK));
    assert!(settings(&profiles, "static:b.png").is_none());
    let mut disabled = profiles;
    disabled[0]["settingsPinned"] = json!(false);
    assert!(settings(&disabled, "static:a.png").is_none());
}
