#![cfg(feature = "daemon")]

use std::fmt::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use serde_json::json;
use skwd_wall_core::{config::Config, material, noctalia, theme, theme_provider};

fn config(root: &Path, authority: &str, mode: &str) -> Config {
    Config::from_root(json!({
        "paths": {"cache": root.join("cache"), "noctaliaBin": root.join("noctalia")},
        "theme": {
            "policy": "wallpaper", "authority": authority, "mode": "light",
            "targets": ["noctalia"], "noctaliaScheme": "m3-content"
        },
        "noctalia": {"themeMode": mode}
    }))
}

fn calls(root: &Path) -> String {
    let path = root.join("calls");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(path, "").unwrap();
    text
}

fn check_modes(root: &Path) {
    for (setting, dark, mode) in [
        ("follow", true, Some("dark")),
        ("follow", false, Some("light")),
        ("keep", true, None),
        ("dark", false, Some("dark")),
        ("light", true, Some("light")),
        ("auto", false, Some("auto")),
        ("invalid", false, None),
    ] {
        assert!(noctalia::activate(&config(root, "noctalia", setting), dark));
        let mut expected = String::from("msg color-scheme-set custom skwd-wall\n");
        if let Some(mode) = mode {
            writeln!(expected, "msg theme-mode-set {mode}").unwrap();
        }
        assert_eq!(calls(root), expected, "{setting}, dark={dark}");
    }
}

fn check_target(root: &Path) {
    let cfg = config(root, "skwd", "follow");
    let mut doc = material::document_with("#854cff", true, "tonal-spot").unwrap();
    let palette = root.join("config/noctalia/palettes/skwd-wall.json");
    theme_provider::publish(&cfg, &doc);
    assert_eq!(calls(root), "msg color-scheme-set custom skwd-wall\nmsg theme-mode-set dark\n");
    let bytes = std::fs::read(&palette).unwrap();
    let modified = std::fs::metadata(&palette).unwrap().modified().unwrap();
    material::select_mode(&mut doc, false);
    theme_provider::publish(&cfg, &doc);
    assert_eq!(calls(root), "msg color-scheme-set custom skwd-wall\nmsg theme-mode-set light\n");
    assert_eq!(std::fs::read(&palette).unwrap(), bytes);
    assert_eq!(std::fs::metadata(&palette).unwrap().modified().unwrap(), modified);
    theme_provider::publish(&config(root, "skwd", "keep"), &doc);
    assert_eq!(calls(root), "msg color-scheme-set custom skwd-wall\n");
    theme_provider::publish(&config(root, "noctalia", "follow"), &doc);
    assert!(calls(root).is_empty());
    let disabled = Config::from_root(json!({
        "paths": {"noctaliaBin": root.join("noctalia")},
        "theme": {"authority": "skwd", "targets": []}
    }));
    theme_provider::publish(&disabled, &doc);
    assert!(calls(root).is_empty());
}

fn check_authority(root: &Path) {
    assert!(theme::apply(&config(root, "noctalia", "follow"), "wallpaper.png"));
    assert_eq!(
        calls(root),
        "theme wallpaper.png --scheme m3-content --both\nmsg color-scheme-set custom skwd-wall\nmsg theme-mode-set light\n"
    );
    let colors = std::fs::read(root.join("cache/colors.json")).unwrap();
    let colors: serde_json::Value = serde_json::from_slice(&colors).unwrap();
    assert_eq!(colors["surface"], "#ffffff");
    assert!(theme::apply(&config(root, "noctalia", "dark"), "wallpaper.png"));
    assert!(calls(root).ends_with("msg theme-mode-set dark\n"));
    let colors: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("cache/colors.json")).unwrap()).unwrap();
    assert_eq!(colors["surface"], "#ffffff");
}

fn check_failures(root: &Path) {
    let cfg = config(root, "noctalia", "follow");
    std::fs::write(root.join("fail"), "color-scheme-set").unwrap();
    assert!(!noctalia::activate(&cfg, false));
    assert_eq!(calls(root), "msg color-scheme-set custom skwd-wall\n");
    std::fs::write(root.join("fail"), "theme-mode-set").unwrap();
    assert!(!noctalia::activate(&cfg, false));
    assert!(calls(root).ends_with("msg theme-mode-set light\n"));
    let palette = root.join("config/noctalia/palettes/skwd-wall.json");
    let before = std::fs::read(&palette).unwrap();
    std::fs::write(root.join("fail"), "theme").unwrap();
    assert!(!theme::apply(&cfg, "wallpaper.png"));
    assert_eq!(calls(root), "theme wallpaper.png --scheme m3-content --both\n");
    assert_eq!(std::fs::read(&palette).unwrap(), before);
    std::fs::remove_file(root.join("fail")).unwrap();
    std::fs::remove_file(&palette).unwrap();
    std::fs::create_dir(&palette).unwrap();
    theme_provider::publish(
        &config(root, "skwd", "follow"),
        &material::document_with("#854cff", true, "tonal-spot").unwrap(),
    );
    assert!(calls(root).is_empty());
}

#[test]
fn noctalia_mode_apply() {
    if let Ok(root) = std::env::var("SKWD_NOCTALIA_MODE_TEST_ROOT") {
        let root = Path::new(&root);
        check_modes(root);
        check_target(root);
        check_authority(root);
        check_failures(root);
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir(root.join("cache")).unwrap();
    let script = root.join("noctalia");
    std::fs::write(
        &script,
        r##"#!/bin/sh
if [ "$1" = --version ]; then printf '%s\n' 'noctalia v5.0.0'; exit 0; fi
printf '%s\n' "$*" >> "$SKWD_NOCTALIA_MODE_TEST_ROOT/calls"
if [ -f "$SKWD_NOCTALIA_MODE_TEST_ROOT/fail" ]; then
    failed=$(cat "$SKWD_NOCTALIA_MODE_TEST_ROOT/fail")
    if [ "$1" = "$failed" ] || [ "$2" = "$failed" ]; then exit 1; fi
fi
if [ "$1" = theme ]; then
    printf '%s\n' '{"dark":{"primary":"#854cff","surface":"#000000"},"light":{"primary":"#854cff","surface":"#ffffff"}}'
fi
"##,
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let out = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "noctalia_mode_apply", "--nocapture"])
        .env("SKWD_NOCTALIA_MODE_TEST_ROOT", root)
        .env("NOCTALIA_CONFIG_HOME", root.join("config"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env_remove("SKWD_WALL_V2_CONFIG")
        .env_remove("SKWD_WALL_V2_CACHE")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
