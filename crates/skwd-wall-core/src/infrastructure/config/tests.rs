use super::ConfigStore;

#[test]
fn store_snapshot() {
    let store = ConfigStore::from_root(serde_json::json!({
        "display": { "fillMode": "fit" }
    }));

    assert_eq!(store.read().display().fill_mode(), "fit");
}

#[test]
fn stationary_wallpaper_ignores_saved_overview_only_pause() {
    for (mode, enabled, expected) in
        [("separate", true, true), ("stationary", true, false), ("stationary", false, true)]
    {
        let store = ConfigStore::from_root(serde_json::json!({
            "niri": { "overviewMode": mode, "overviewBackdrop": enabled, "overviewOnlyPlayback": true },
            "playback": { "fullscreen": true, "processEnabled": true }
        }));
        let config = store.read();
        assert_eq!(config.playback().overview_only(), expected);
        assert!(config.playback().fullscreen_pause());
    }
}
