#![cfg(test)]

use super::*;

fn matugen_fixture() -> Value {
    serde_json::json!({
        "base16": {},
        "image": "/w/a.jpg",
        "is_dark_mode": true,
        "mode": "dark",
        "palettes": {},
        "colors": {
            "primary": {
                "dark": {"color": "#ffb3b0"},
                "light": {"color": "#904a49"},
                "default": {"color": "#ffb3b0"}
            },
            "on_primary": {
                "dark": {"color": "#571d1c"},
                "light": {"color": "#ffffff"},
                "default": {"color": "#571d1c"}
            },
            "surface": {
                "dark": {"color": "#181211"},
                "light": {"color": "#fff8f7"},
                "default": {"color": "#181211"}
            }
        }
    })
}

#[test]
fn paths_xdg_fallback() {
    assert_eq!(
        colors_path_from(Some("/xdg-cache"), "/home/u"),
        PathBuf::from("/xdg-cache/DankMaterialShell/dms-colors.json")
    );
    assert_eq!(
        colors_path_from(None, "/home/u"),
        PathBuf::from("/home/u/.cache/DankMaterialShell/dms-colors.json"),
    );
    assert_eq!(
        settings_path_from(Some(""), "/home/u"),
        PathBuf::from("/home/u/.config/DankMaterialShell/settings.json")
    );
}

#[test]
fn scheme_fallback() {
    let settings = serde_json::json!({"matugenScheme": "scheme-vibrant"});
    assert_eq!(scheme_from_settings(Some(&settings)), "scheme-vibrant");
    assert_eq!(scheme_from_settings(Some(&serde_json::json!({}))), "scheme-tonal-spot");
    assert_eq!(scheme_from_settings(None), "scheme-tonal-spot");
    assert_eq!(
        scheme_from_settings(Some(&serde_json::json!({"matugenScheme": ""}))),
        "scheme-tonal-spot"
    );
}

#[test]
fn imports_dms_variant() {
    let shell = serde_json::json!({
        "colors": {
            "dark": { "primary": "#111111", "surface": "#222222" },
            "light": { "primary": "#eeeeee", "surface": "#ffffff" }
        }
    });
    assert_eq!(
        imported_shell_colors(&shell, true),
        Some(serde_json::json!({ "primary": "#111111", "surface": "#222222" }))
    );
    assert_eq!(
        imported_shell_colors(&shell, false),
        Some(serde_json::json!({ "primary": "#eeeeee", "surface": "#ffffff" }))
    );
    assert!(imported_shell_colors(&serde_json::json!({}), true).is_none());
}

#[test]
fn matugen_args_index() {
    let args = matugen_gen_args("/w/a.jpg", "scheme-vibrant", 0);
    assert_eq!(
        args,
        vec![
            "image",
            "/w/a.jpg",
            "--dry-run",
            "-j",
            "hex",
            "-t",
            "scheme-vibrant",
            "--source-color-index",
            "0"
        ],
    );
}

#[test]
fn tokens_to_dank_json() {
    let tokens = matugen_fixture();
    let dank = dank_colors_json(&tokens).expect("dark+light present");
    assert_eq!(dank["colors"]["dark"]["primary"], "#ffb3b0");
    assert_eq!(dank["colors"]["light"]["primary"], "#904a49");
    assert_eq!(dank["colors"]["dark"]["on_primary"], "#571d1c");
    let flat_dark = flat_mode_colors(&tokens, true).expect("dark colors");
    assert_eq!(flat_dark["primary"], "#ffb3b0");
    assert_eq!(flat_dark["surface"], "#181211");
    let flat_light = flat_mode_colors(&tokens, false).expect("light colors");
    assert_eq!(flat_light["primary"], "#904a49");
    assert!(dank_colors_json(&serde_json::json!({"colors": {}})).is_none());
}

#[test]
fn preview_restore_noop() {
    let dir = std::env::temp_dir().join(format!("skwd-dms-prev-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = Config::from_root(serde_json::json!({"paths": {"cache": dir.to_string_lossy()}}));
    let backup = PathBuf::from(cfg.cache_dir()).join("dms-preview-orig.json");
    assert!(!backup.exists());
    restore_stale_preview(&cfg);
    assert!(!backup.exists());
    let _ = std::fs::remove_dir_all(&dir);
}
