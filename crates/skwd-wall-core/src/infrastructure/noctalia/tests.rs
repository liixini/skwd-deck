#![cfg(test)]

use super::*;

#[test]
fn bin_resolution_order() {
    let missing = std::path::Path::new("/definitely/not/here/noctalia");
    assert_eq!(bin_from("/opt/noctalia/bin/noctalia", missing), "/opt/noctalia/bin/noctalia");
    assert_eq!(bin_from("", missing), "noctalia");
    let dir = std::env::temp_dir().join(format!("skwd-noctalia-bin-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let local = dir.join("noctalia");
    std::fs::write(&local, b"x").unwrap();
    assert_eq!(bin_from("", &local), local.to_string_lossy(),);
    assert_eq!(bin_from("/cfg/noctalia", &local), "/cfg/noctalia");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn parse_scheme_fallback() {
    assert_eq!(parse_scheme("wallpaper m3-tonal-spot"), "m3-tonal-spot");
    assert_eq!(parse_scheme("wallpaper vibrant\n"), "vibrant");
    assert_eq!(parse_scheme("builtin Noctalia"), FALLBACK_SCHEME);
    assert_eq!(parse_scheme("custom skwd-hover"), FALLBACK_SCHEME);
    assert_eq!(parse_scheme(""), FALLBACK_SCHEME);
    assert_eq!(parse_scheme("garbage"), FALLBACK_SCHEME);
}

#[test]
fn theme_gen_args_shape() {
    assert_eq!(
        theme_gen_args("/w/a.jpg", "m3-content", "--dark", false),
        vec!["theme", "/w/a.jpg", "--scheme", "m3-content", "--dark"]
    );
    assert_eq!(
        theme_gen_args("/w/a.jpg", "vibrant", "--both", false),
        vec!["theme", "/w/a.jpg", "--scheme", "vibrant", "--both"]
    );
    assert_eq!(
        theme_gen_args("/w/a.jpg", "m3-tonal-spot", "--dark", true),
        vec!["theme", "/w/a.jpg", "--scheme", "m3-tonal-spot", "--dark", "--pure-black"]
    );
}

#[test]
fn restore_args_scheme() {
    assert_eq!(
        restore_args(("wallpaper".to_string(), "m3-content".to_string())),
        vec!["msg", "color-scheme-set", "wallpaper", "m3-content"]
    );
    assert_eq!(
        restore_args(("custom".to_string(), PREVIEW_SCHEME.to_string())),
        vec!["msg", "color-scheme-set", "custom", "skwd-hover"]
    );
}

#[test]
fn palettes_dir_order() {
    assert_eq!(
        palettes_dir_from(Some("/custom"), Some("/xdg"), "/home/u"),
        PathBuf::from("/custom/noctalia/palettes")
    );
    assert_eq!(
        palettes_dir_from(None, Some("/xdg"), "/home/u"),
        PathBuf::from("/xdg/noctalia/palettes")
    );
    assert_eq!(
        palettes_dir_from(Some(""), Some(""), "/home/u"),
        PathBuf::from("/home/u/.config/noctalia/palettes")
    );
}

#[test]
fn cli_tokens_palette() {
    let mode = serde_json::json!({
        "primary": "#ffb3b0", "on_primary": "#571d1c",
        "secondary": "#e7bdbb", "on_secondary": "#442a29",
        "tertiary": "#e0c38c", "on_tertiary": "#3f2e04",
        "error": "#ffb4ab", "on_error": "#690005",
        "surface": "#181211", "on_surface": "#eedfde",
        "surface_variant": "#534342", "on_surface_variant": "#d8c2c0",
        "outline": "#a08c8b", "shadow": "#000000",
        "terminal_foreground": "#eedfde", "terminal_background": "#181211",
        "terminal_selection_fg": "#181211", "terminal_selection_bg": "#eedfde",
        "terminal_cursor_text": "#181211", "terminal_cursor": "#eedfde",
        "terminal_normal_black": "#272120", "terminal_normal_red": "#ffb3b0",
        "terminal_normal_green": "#b8ccb0", "terminal_normal_yellow": "#e0c38c",
        "terminal_normal_blue": "#b0c6ff", "terminal_normal_magenta": "#e7bdbb",
        "terminal_normal_cyan": "#a0cfcb", "terminal_normal_white": "#d8c2c0",
        "terminal_bright_black": "#3f3231", "terminal_bright_red": "#ffdad8",
        "terminal_bright_green": "#d4e8cb", "terminal_bright_yellow": "#fddfa6",
        "terminal_bright_blue": "#d8e2ff", "terminal_bright_magenta": "#ffdad8",
        "terminal_bright_cyan": "#bcebe7", "terminal_bright_white": "#f5dedd"
    });
    let cli = serde_json::json!({"dark": mode, "light": mode});
    let out = cli_tokens_to_custom_palette(&cli).expect("dark+light transform");
    let dark = &out["dark"];
    assert_eq!(dark["mPrimary"], "#ffb3b0");
    assert_eq!(dark["mOnSurfaceVariant"], "#d8c2c0");
    assert_eq!(dark["mHover"], dark["mTertiary"]);
    assert_eq!(dark["terminal"]["normal"]["red"], "#ffb3b0");
    assert_eq!(dark["terminal"]["bright"]["cyan"], "#bcebe7");
    assert_eq!(dark["terminal"]["selectionFg"], "#181211");
    assert!(dark["terminal"].get("cursorText").is_some_and(|val| val == "#181211"));
    assert!(out["light"].is_object());
    assert!(cli_tokens_to_custom_palette(&serde_json::json!({"dark": mode})).is_none());
}

#[test]
fn palette_cache_bounds() {
    let st = WallState::test_new(serde_json::json!({}));
    assert!(st.theme().shell_palette_cached("k").is_none());
    st.theme().cache_shell_palette("k".to_string(), b"json".to_vec());
    assert_eq!(st.theme().shell_palette_cached("k").as_deref(), Some(b"json".as_ref()),);
    for idx in 0..130 {
        st.theme().cache_shell_palette(format!("fill-{idx}"), vec![0]);
    }
    assert!(st.theme().shell_palette_cached("k").is_none());
}

#[test]
fn preview_state_restore() {
    let st = WallState::test_new(serde_json::json!({}));
    assert!(st.theme().noctalia_preview_orig().is_none());
    let generation = st.theme().bump_shell_preview();
    assert_eq!(st.theme().shell_preview_generation(), generation);
    st.theme().set_noctalia_preview_orig(("wallpaper".to_string(), "m3-content".to_string()));
    assert!(st.theme().noctalia_preview_orig().is_some());
    let taken = st.theme().take_noctalia_preview_orig();
    assert_eq!(taken, Some(("wallpaper".to_string(), "m3-content".to_string())));
    assert!(st.theme().take_noctalia_preview_orig().is_none());
}

#[test]
fn sanitize_skips_preview() {
    assert_eq!(
        sanitize_orig_scheme("wallpaper m3-content"),
        (String::from("wallpaper"), String::from("m3-content")),
    );
    assert_eq!(
        sanitize_orig_scheme(&format!("custom {PREVIEW_SCHEME}")),
        (String::from("wallpaper"), String::from(FALLBACK_SCHEME)),
    );
    assert_eq!(
        sanitize_orig_scheme(""),
        (String::from("wallpaper"), String::from(FALLBACK_SCHEME)),
    );
}

fn marker_config(dir: &std::path::Path) -> Config {
    Config::from_root(serde_json::json!({
        "paths": { "cache": dir.to_string_lossy() },
        "noctalia": { "bin": dir.join("noctalia-does-not-exist").to_string_lossy() },
    }))
}

#[test]
fn unreadable_marker_kept() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = marker_config(tmp.path());
    let marker = preview_orig_path(&cfg);
    std::fs::write(&marker, b"not json at all").unwrap();
    restore_stale_preview(&cfg);
    assert!(marker.exists());
}

#[test]
fn failed_restore_keeps_marker() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = marker_config(tmp.path());
    let marker = preview_orig_path(&cfg);
    std::fs::write(&marker, br#"{"source":"wallpaper","name":"m3-content"}"#).unwrap();
    restore_stale_preview(&cfg);
    assert!(marker.exists());
}

#[test]
fn failed_end_keeps_state() {
    let tmp = tempfile::tempdir().unwrap();
    let st = WallState::test_new(serde_json::json!({
        "paths": { "cache": tmp.path().to_string_lossy() },
        "noctalia": { "bin": tmp.path().join("noctalia-does-not-exist").to_string_lossy() },
    }));
    let orig = ("wallpaper".to_string(), "m3-content".to_string());
    st.theme().set_noctalia_preview_orig(orig.clone());
    preview_end(&st);
    assert_eq!(st.theme().noctalia_preview_orig(), Some(orig));
}
