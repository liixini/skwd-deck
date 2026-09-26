use super::canonicalize_depth_layout;
use serde_json::json;

#[test]
fn converts_percent_settings_and_presets_once() {
    let mut config = json!({"motion":{"standardMs":400},"components":{"wallpaperSelector":{
        "depthHeight":600,"depthWidth":50,"depthSpacing":80,"depthFalloff":25,"depthSpeed":200,
        "presets":{"depth":[{"name":"Old", "params":{"depthHeight":400,"depthWidth":60,"depthSpacing":100,"depthFalloff":50,"depthSpeed":50}}]}
    }}});
    canonicalize_depth_layout(&mut config);
    let selector = &config["components"]["wallpaperSelector"];
    assert_eq!(selector["depthWidthPx"], json!(300.0));
    assert_eq!(selector["depthSpacingPx"], json!(240.0));
    assert_eq!(selector["depthFalloffFactor"], json!(0.25));
    assert_eq!(selector["depthNavigationMs"], json!(200.0));
    assert!(selector.get("depthWidth").is_none());
    let preset = &selector["presets"]["depth"][0]["params"];
    assert_eq!(preset["depthWidthPx"], json!(240.0));
    assert_eq!(preset["depthNavigationMs"], json!(800.0));
    let once = config.clone();
    canonicalize_depth_layout(&mut config);
    assert_eq!(config, once);
    config["components"]["wallpaperSelector"]["depthHeight"] = json!(900);
    canonicalize_depth_layout(&mut config);
    assert_eq!(config["components"]["wallpaperSelector"]["depthWidthPx"], json!(300.0));
}

#[test]
fn explicit_absolute_values_win_over_legacy_values() {
    let mut config = json!({"components":{"wallpaperSelector":{"depthWidth":50,"depthWidthPx":450,"depthNavigationMs":600,"depthSpeed":100}}});
    canonicalize_depth_layout(&mut config);
    assert_eq!(config["components"]["wallpaperSelector"]["depthWidthPx"], json!(450));
    assert_eq!(config["components"]["wallpaperSelector"]["depthNavigationMs"], json!(600));
}
