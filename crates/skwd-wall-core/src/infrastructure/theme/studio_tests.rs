use serde_json::{Value, json};

use super::*;
use skwd_palette::studio::{Role, SHADE_RANGE};

#[test]
fn studio_time_modes() {
    let image = "/wall.png";
    let config = Config::from_root(json!({"theme": {"studioMode": "by-time"}}));
    for (minute, dark) in [(419, true), (420, false), (1139, false), (1140, true)] {
        assert_eq!(mode_dark_at(&config, image, minute), dark);
    }
    for (mode, dark) in [("dark", true), ("light", false)] {
        let config = Config::from_root(json!({"theme": {"studioMode": mode}}));
        assert_eq!(mode_dark_at(&config, image, 420), dark);
        assert_eq!(mode_dark_at(&config, image, 1140), dark);
    }
}

#[test]
fn reads_tuning() {
    let config = Config::from_root(json!({"theme": {
        "studioWarmth": 0.5,
        "studioColour": 9.0,
        "studioContrast": -0.25,
        "studioShades": [0, 0, 0, 0, 0, 7, 0, -1],
    }}));
    let tuning = tuning(&config);
    assert_eq!(tuning.warmth, 0.5);
    assert_eq!(tuning.colour, 1.6);
    assert_eq!(tuning.contrast, -0.25);
    assert_eq!(tuning.shades[Role::Accent.index()], SHADE_RANGE.1);
    assert_eq!(tuning.shades[Role::Second.index()], -1);
    assert_eq!(tuning.shades[Role::Third.index()], 0);
}

#[test]
fn theme_needs_no_image() {
    let config = Config::from_root(json!({"theme": {"studioMethod": "theme:catppuccin-latte"}}));
    let (palette, dark) = palette(&config, "/does/not/exist.png").expect("palette");
    assert!(!dark);
    assert_eq!(palette["surface"], "#eff1f5");
    assert!(palette["ansi_red"].is_string());
}

#[test]
fn area_argument() {
    assert_eq!(
        area_arg(StudioArea { left: 0.25, top: 0.5, right: 0.75, bottom: 1.0 }),
        "0.25,0.5,0.75,1"
    );
}

#[test]
fn studio_mode_wins() {
    let config =
        |mode: &str| Config::from_root(json!({"theme": {"mode": "light", "studioMode": mode}}));
    assert!(mode_dark(&config("dark"), "/does/not/exist.png"));
    assert!(!mode_dark(&config("light"), "/does/not/exist.png"));
    assert!(!mode_dark(&config(""), "/does/not/exist.png"));
}

#[test]
fn methods_need_a_scan() {
    let config = Config::from_root(json!({"theme": {"studioMethod": "tonal"}}));
    assert!(palette(&config, "/does/not/exist.png").is_none());
}

#[test]
fn reads_studio_area() {
    let config = |area: Value| Config::from_root(json!({"theme": {"studioArea": area}}));
    let area = config(json!([0.1, 0.2, 0.6, 0.9])).theme().studio_area().expect("area");
    assert_eq!((area.left, area.bottom), (0.1, 0.9));
    assert!(config(json!([0.6, 0.2, 0.1, 0.9])).theme().studio_area().is_none());
    assert!(config(json!([0.1, 0.2])).theme().studio_area().is_none());
}

#[test]
fn area_needs_box() {
    let config = Config::from_root(json!({"theme": {"studioMethod": "area"}}));
    assert!(palette(&config, "/does/not/exist.png").is_none());
}

#[test]
fn area_follows_its_image() {
    let config = Config::from_root(json!({"theme": {
        "studioMethod": "area",
        "studioArea": [0.1, 0.2, 0.6, 0.9],
        "studioAreaImage": "/w/dusk.webp",
    }}));
    let (source, area) = picked(&config, "/w/dusk.webp");
    assert_eq!(source, Source::Method(AREA_KEY.into()));
    assert!(area.is_some());
    let (source, area) = picked(&config, "/w/other.webp");
    assert_eq!(source, Source::Method(Method::Tonal.key().into()));
    assert!(area.is_none());
}

#[test]
fn plain_reuses_rich_scan() {
    let image = "/does/not/exist/rich.webp";
    let rich =
        StudioResult { image: image.into(), seed: "#336699".into(), ..StudioResult::default() };
    lock(cache()).insert(cache_key(image, None, true), rich.clone());
    assert_eq!(snapshot(image, None, false), Some(rich));
}

#[test]
fn schemes_reuse_plain_scan() {
    let image = "/does/not/exist/plain.webp";
    let plain =
        StudioResult { image: image.into(), seed: "#663399".into(), ..StudioResult::default() };
    lock(cache()).insert(cache_key(image, None, false), plain.clone());
    let rich = snapshot(image, None, true).expect("rich");
    assert_eq!(rich.seed, plain.seed);
}
