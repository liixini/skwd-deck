#![cfg(test)]

use super::*;
use serde_json::json;

#[test]
fn swatch_order() {
    let palette = json!({
        "primary": "#111111",
        "tertiary": "#222222",
        "surfaceVariant": "#333333",
        "surfaceContainer": "#444444",
        "surface": "#555555",
        "outline": "#666666",
        "unrelated": "#777777",
    });
    let cols = swatch_from_palette(&palette);
    assert_eq!(cols, ["#111111", "#222222", "#333333", "#444444", "#555555", "#666666"]);

    let bridge = picker_palette_json(&cols).expect("six colour bridge");
    assert_eq!(bridge["primary"], "#111111");
    assert_eq!(bridge["surface"], "#555555");
    assert_eq!(bridge["background"], "#555555");
    assert_eq!(bridge["outline"], "#666666");
}

#[test]
fn swatch_missing_keys() {
    assert!(swatch_from_palette(&json!({"primary": "#111111"})).len() < 6);
    assert!(picker_palette_json(&swatch_from_palette(&json!({}))).is_none());
}

#[test]
fn backend_defaults_ignore_matugen() {
    let default = Config::from_root(json!({}));
    assert_eq!(default.theme().backend(), "skwd-iris");
    let matugen_disabled = Config::from_root(json!({ "features": { "matugen": false } }));
    assert_eq!(matugen_disabled.theme().backend(), "skwd-iris");
    let explicit = Config::from_root(
        json!({ "theme": { "backend": "native" }, "features": { "matugen": false } }),
    );
    assert_eq!(explicit.theme().backend(), "native");
}

#[test]
fn wallust_command_modes() {
    let dflt = Config::from_root(json!({}));
    assert_eq!(wallust_command(&dflt, "/a b.png", true), "wallust run '/a b.png'");
    assert_eq!(wallust_command(&dflt, "/a.png", false), "wallust run -p light16 '/a.png'");
    let ov = Config::from_root(json!({ "theme": { "wallustPalette": "harddark" } }));
    assert_eq!(wallust_command(&ov, "/a.png", true), "wallust run -p 'harddark' '/a.png'");
    let cs = Config::from_root(
        json!({ "theme": { "wallustPalette": "softdark", "wallustColorspace": "labmixed" } }),
    );
    assert_eq!(
        wallust_command(&cs, "/a.png", true),
        "wallust run -p 'softdark' --colorspace 'labmixed' '/a.png'"
    );
}

#[test]
fn pywal_command_flags() {
    let dflt = Config::from_root(json!({}));
    assert_eq!(pywal_command(&dflt, "/a.png", true), "wal -q -n -i '/a.png'");
    assert_eq!(pywal_command(&dflt, "/a.png", false), "wal -q -n -i '/a.png' -l");
    let sat = Config::from_root(json!({ "theme": { "pywalSaturate": "0.6" } }));
    assert_eq!(pywal_command(&sat, "/a.png", true), "wal -q -n --saturate '0.6' -i '/a.png'");
    let bad = Config::from_root(json!({ "theme": { "pywalSaturate": "2.5" } }));
    assert_eq!(pywal_command(&bad, "/a.png", true), "wal -q -n -i '/a.png'",);
}

#[test]
fn picker_palette_bridge() {
    let cols: Vec<String> = ["#ff0000", "#00ff00", "#333333", "#222222", "#101010", "#808080"]
        .iter()
        .map(|hex| (*hex).to_string())
        .collect();
    let bridge = picker_palette_json(&cols).expect("six colours bridge");
    assert_eq!(bridge["primary"], "#ff0000");
    assert_eq!(bridge["surface"], "#101010");
    assert_eq!(bridge["background"], "#101010");
    assert_eq!(bridge["surfaceText"], "#f2f2f2");
    assert_eq!(bridge["primaryText"], "#f2f2f2");
    assert!(picker_palette_json(&cols[..5]).is_none());
}

#[test]
fn theme_mode_fallback() {
    let inherit = Config::from_root(json!({ "matugen": { "mode": "light" } }));
    assert_eq!(inherit.theme().mode(), "light");
    let explicit =
        Config::from_root(json!({ "theme": { "mode": "dark" }, "matugen": { "mode": "light" } }));
    assert_eq!(explicit.theme().mode(), "dark");
    let dflt = Config::from_root(json!({}));
    assert_eq!(dflt.theme().mode(), "dark");
}

#[test]
fn wallust_pick_slots() {
    let lines: Vec<String> = (0..16).map(|idx| format!("#0000{idx:02x}")).collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    assert_eq!(
        wallust_pick(&refs),
        vec!["#000001", "#000002", "#000004", "#000008", "#000000", "#000007"]
    );
    assert!(wallust_pick(&refs[..15]).is_empty());
}

#[test]
fn wallust_pick_noise() {
    use std::fmt::Write as _;
    let mut text = String::from("wallust: applied templates\nscheme: base16\n");
    for idx in 0..16 {
        let _ = writeln!(text, "  #0000{idx:02x}  ");
    }
    text.push_str("done\n");
    assert_eq!(
        wallust_pick_from_text(&text),
        vec!["#000001", "#000002", "#000004", "#000008", "#000000", "#000007"],
    );
    assert!(wallust_pick_from_text("nothing here").is_empty());
}

#[test]
fn pywal_pick_colors() {
    let mut colors = serde_json::Map::new();
    for idx in 0..16 {
        colors.insert(format!("color{idx}"), json!(format!("#0000{idx:02x}")));
    }
    let doc = json!({ "special": { "background": "#111111" }, "colors": colors });
    assert_eq!(
        pywal_pick(&doc),
        vec!["#000001", "#000004", "#000002", "#000006", "#111111", "#000008"]
    );
}

#[test]
fn matugen_dry_run_args() {
    let dflt = Config::from_root(json!({}));
    let got = matugen_preview_args(&dflt, "/a b.png", true);
    let got: Vec<&str> = got.iter().map(String::as_str).collect();
    assert_eq!(
        got,
        vec![
            "image",
            "/a b.png",
            "--dry-run",
            "-j",
            "hex",
            "-t",
            "scheme-fidelity",
            "-m",
            "dark",
            "--source-color-index",
            "0",
        ]
    );
    let custom = Config::from_root(
        json!({ "matugen": { "schemeType": "scheme-vibrant", "colorIndex": 2 } }),
    );
    let got = matugen_preview_args(&custom, "/x.png", false);
    let got: Vec<&str> = got.iter().map(String::as_str).collect();
    assert_eq!(
        got,
        vec![
            "image",
            "/x.png",
            "--dry-run",
            "-j",
            "hex",
            "-t",
            "scheme-vibrant",
            "-m",
            "light",
            "--source-color-index",
            "2",
        ]
    );
}

#[test]
fn static_palette_saved() {
    let cfg = Config::from_root(json!({
        "theme": {
            "staticTheme": "My Custom",
            "savedThemes": [{
                "name": "My Custom",
                "primary": "#b48ead", "primaryText": "#2e3440", "tertiary": "#a3be8c",
                "surface": "#2e3440", "surfaceText": "#d8dee9", "surfaceVariant": "#3b4252",
                "surfaceContainer": "#434c5e", "background": "#242933", "outline": "#4c566a"
            }]
        }
    }));
    let val = static_palette_value(&cfg, true).expect("saved theme resolves");
    assert_eq!(val["primary"], "#b48ead");
    assert_eq!(val["on_primary"], "#2e3440");
    assert_eq!(val["on_surface"], "#d8dee9");
    assert!(val.get("name").is_none());
    assert_eq!(swatch_from_palette(&val).len(), 6);

    let missing = Config::from_root(json!({"theme": {"staticTheme": "No Such"}}));
    assert!(static_palette_value(&missing, true).is_none());

    let curated = Config::from_root(json!({"theme": {"staticTheme": "kanagawa"}}));
    let val = static_palette_value(&curated, true).expect("curated presets resolve as statics");
    assert_eq!(swatch_from_palette(&val).len(), 6);
    assert_eq!(val["primary"], "#7e9cd8");
}

#[test]
fn matugen_pick_roles() {
    let doc = json!({
        "colors": {
            "primary": { "default": { "color": "#111111" } },
            "tertiary": { "default": { "color": "#222222" } },
            "surface_variant": { "default": { "color": "#333333" } },
            "surface_container": { "default": { "color": "#444444" } },
            "surface": { "default": { "color": "#555555" } },
            "outline": { "default": { "color": "#666666" } }
        }
    });
    assert_eq!(
        matugen_pick(&doc),
        vec!["#111111", "#222222", "#333333", "#444444", "#555555", "#666666"]
    );
    assert!(matugen_pick(&json!({})).is_empty());

    let bridge = picker_palette_json(&matugen_pick(&doc)).expect("picker bridge");
    assert_eq!(bridge["primary"], "#111111");
    assert_eq!(bridge["surface"], "#555555");
    assert_eq!(bridge["outline"], "#666666");
}

#[test]
fn builtins_available() {
    let cfg = Config::from_root(serde_json::json!({}));
    assert!(backend_available(&cfg, "native"));
    assert!(backend_available(&cfg, "off"));
    assert!(!backend_available(&cfg, "bogus"));
}

#[test]
fn effective_backend_fallback() {
    let off = Config::from_root(json!({ "theme": { "backend": "off" } }));
    assert_eq!(effective_backend(&off), "off");
    let native = Config::from_root(json!({ "theme": { "backend": "native" } }));
    assert_eq!(effective_backend(&native), "native");
    let bogus = Config::from_root(json!({ "theme": { "backend": "not-a-backend" } }));
    assert_eq!(effective_backend(&bogus), "native");
}

#[test]
fn probe_cache_ttl() {
    let cache = Mutex::new(HashMap::new());
    let mut probes = 0;
    assert!(probe_cached(&cache, "matugen", || {
        probes += 1;
        true
    }));
    assert!(probe_cached(&cache, "matugen", || {
        probes += 1;
        false
    }));
    assert_eq!(probes, 1);
    let mut other = 0;
    assert!(!probe_cached(&cache, "wallust", || {
        other += 1;
        false
    }));
    assert_eq!(other, 1);
}

#[test]
fn available_backends_known() {
    let cfg = Config::from_root(json!({}));
    let got = available_backends(&cfg);
    assert!(got.contains(&"off") && got.contains(&"native"));
    assert!(got.iter().all(|backend| ALL_BACKENDS.contains(backend)));
    let mut sorted = got.clone();
    sorted.sort_by_key(|backend| ALL_BACKENDS.iter().position(|known| known == backend));
    assert_eq!(got, sorted);
}

#[test]
fn hex_parsers_multibyte() {
    assert_eq!(parse_seed("#aéabc"), None);
    assert_eq!(parse_seed("#éééééé"), None);
    for hex in ["#aéabc", "é", "", "#éééééé", "#ff88"] {
        let luma = hex_luma(hex);
        assert!(luma.is_finite() && (0.0..=1.0).contains(&luma), "{hex} -> {luma}");
    }
    assert_eq!(hex_luma("é"), 0.0);
    assert_eq!(hex_luma(""), 0.0);
    assert_eq!(parse_seed("#ff8800"), Some(skwd_palette::Rgb(255, 136, 0)));
    assert!(hex_luma("#ffffff") > hex_luma("#000000"));
}

#[test]
fn write_scheme_document() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = Config::from_root(serde_json::json!({
        "paths": { "cache": tmp.path().to_string_lossy() }
    }));
    assert!(write_scheme(&cfg, "#f06e44", true));
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(scheme_path(&cfg)).unwrap()).unwrap();
    assert_eq!(doc["mode"], "dark");
    assert_eq!(doc["colors"]["primary"]["dark"]["color"], "#ffb59e");
    assert!(doc["colors"].as_object().unwrap().len() >= 50);
}

#[test]
fn write_scheme_bad_seed() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = Config::from_root(serde_json::json!({
        "paths": { "cache": tmp.path().to_string_lossy() }
    }));
    for bad in ["", "not-a-colour", "#aéabc", "#12345"] {
        assert!(!write_scheme(&cfg, bad, true), "{bad}");
    }
    assert!(!scheme_path(&cfg).exists());
}

#[test]
fn iris_args_shape() {
    assert_eq!(iris_args("/w/a.png", true, false), vec!["/w/a.png", "--dark", "1"]);
    assert_eq!(iris_args("/w/a.png", false, false), vec!["/w/a.png", "--dark", "0"]);
    assert_eq!(iris_args("/w/a.png", true, true), vec!["/w/a.png", "--dark", "1", "--json-only"]);
}

#[test]
fn iris_pick_roles() {
    let val = json!({
        "bg": "#101010", "surface": "#202020", "fg": "#eeeeee", "dim": "#707070",
        "accent": "#ff8800", "green": "#00cc44", "yellow": "#ffcc00", "dark": true
    });
    assert_eq!(
        iris_pick(&val),
        vec!["#ff8800", "#00cc44", "#202020", "#202020", "#101010", "#707070"]
    );
    assert!(iris_pick(&json!({"accent": "#ff8800"})).is_empty());
    assert!(iris_pick(&json!({})).is_empty());
}

#[test]
fn iris_pick_optional_fallback() {
    let val = json!({"bg": "#101010", "surface": "#202020", "accent": "#ff8800"});
    assert_eq!(
        iris_pick(&val),
        vec!["#ff8800", "#ff8800", "#202020", "#202020", "#101010", "#202020"]
    );
}

#[test]
fn palette_scheme_contrast() {
    let doc = crate::material::document("#f06e44", true).unwrap();
    let palette = palette_from_scheme(&doc).expect("core roles present");
    assert_eq!(palette["primary"], "#ffb59e");
    assert_eq!(palette["primaryText"], "#561f0e");
    assert_eq!(palette["surfaceText"], doc["colors"]["on_surface"]["default"]["color"]);
    for guessed in ["#1a1a1a", "#f2f2f2"] {
        assert_ne!(palette["primaryText"], guessed);
    }
    assert_eq!(swatch_from_palette(&palette).len(), 6);
}

#[test]
fn publish_scheme_authorities() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = Config::from_root(json!({ "paths": { "cache": tmp.path().to_string_lossy() } }));
    assert!(publish_scheme(&cfg, "#f06e44", true));
    let scheme: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(scheme_path(&cfg)).unwrap()).unwrap();
    let colors: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(tmp.path().join("colors.json")).unwrap())
            .unwrap();
    assert_eq!(colors["primary"], scheme["colors"]["primary"]["default"]["color"]);
    assert_eq!(colors["outline"], scheme["colors"]["outline"]["default"]["color"]);
    assert!(!publish_scheme(&cfg, "bogus", true));
}

#[test]
fn unavailable_backend_fallback() {
    let cfg = Config::from_root(json!({ "theme": { "backend": "iris" } }));
    if backend_available(&cfg, "iris") {
        return;
    }
    assert_eq!(effective_backend(&cfg), "native");
}

fn contrast_of(lhs: &str, rhs: &str) -> f32 {
    let parse = |hex: &str| {
        let digits = hex.trim_start_matches('#');
        let chan = |idx: usize| u8::from_str_radix(&digits[idx..idx + 2], 16).unwrap();
        skwd_palette::Rgb(chan(0), chan(2), chan(4))
    };
    skwd_palette::semantic::contrast(parse(lhs), parse(rhs))
}

#[test]
fn semantic_palette_tonal_layers() {
    for (label, val) in [
        (
            "dark",
            json!({"bg":"#504e3f","surface":"#5a5847","fg":"#e9ddcb","dim":"#8a8570","accent":"#f1d082"}),
        ),
        (
            "light",
            json!({"bg":"#dceaf6","surface":"#cfe0ef","fg":"#161a1f","dim":"#7d8f9e","accent":"#3f5683"}),
        ),
    ] {
        let (palette, _) = semantic_palette(&val).expect("all roles present");
        let get = |key: &str| palette[key].as_str().unwrap().to_string();
        let step = contrast_of(&get("surface"), &get("surfaceContainer"));
        assert!(step >= 1.45, "{label}: contrast {step:.2}");
        for pair in [
            ("surface", "surfaceVariant"),
            ("surfaceVariant", "surfaceContainer"),
            ("surface", "background"),
        ] {
            assert_ne!(palette[pair.0], palette[pair.1], "{label}: {} == {}", pair.0, pair.1);
        }
        assert!(contrast_of(&get("surfaceText"), &get("surface")) >= 4.5, "{label}");
        assert_eq!(get("surface"), val["bg"].as_str().unwrap(), "{label}");
    }
}

#[test]
fn semantic_palette_second_hue() {
    let val = json!({
        "bg": "#101010", "surface": "#202020", "fg": "#eeeeee",
        "dim": "#707070", "accent": "#ff8800", "green": "#33cc55"
    });
    let (palette, _) = semantic_palette(&val).unwrap();
    assert_eq!(palette["tertiary"], "#33cc55");
}

#[test]
fn explicit_modes_skip_probe() {
    let dark = Config::from_root(json!({ "matugen": { "mode": "dark" } }));
    assert!(resolve_dark(&dark, "/definitely/not/a/real/image.png"));
    let light = Config::from_root(json!({ "matugen": { "mode": "light" } }));
    assert!(!resolve_dark(&light, "/definitely/not/a/real/image.png"));
}

#[test]
fn auto_mode_unreadable_dark() {
    let auto = Config::from_root(json!({ "matugen": { "mode": "auto" } }));
    assert!(resolve_dark(&auto, "/definitely/not/a/real/image.png"));
}

#[test]
fn auto_tone_cached() {
    let tmp = tempfile::tempdir().unwrap();
    let img = tmp.path().join("probe-me.png");
    std::fs::write(&img, b"not really an image").unwrap();
    let auto = Config::from_root(json!({ "matugen": { "mode": "auto" } }));
    let path = img.to_string_lossy().to_string();
    let first = resolve_dark(&auto, &path);
    for _ in 0..50 {
        assert_eq!(resolve_dark(&auto, &path), first);
    }
}

fn styled_config(cache: &std::path::Path, style: &str) -> Config {
    Config::from_root(json!({
        "paths": { "cache": cache.to_string_lossy() },
        "theme": { "style": style },
    }))
}

#[test]
fn preview_apply_same_seed() {
    for style in ["natural", "pastel", "muted", "vibrant"] {
        for dark in [true, false] {
            let tmp = tempfile::tempdir().unwrap();
            let config = styled_config(tmp.path(), style);
            let preview =
                styled_palette(&config, "#f06e44", dark).expect("a valid seed yields a palette");
            let doc = crate::material::document_with("#f06e44", dark, &config.theme().scheme())
                .expect("scheme document");
            let applied =
                write_picker_palette_value(&config, &palette_from_scheme(&doc).expect("roles"));
            assert_eq!(preview, applied, "{style}/dark={dark}");
        }
    }
}

#[test]
fn skwd_iris_harmonises_dominant() {
    let iris = json!({
        "bg": "#4e533c", "surface": "#61684b", "fg": "#fae3aa",
        "dim": "#83896f", "accent": "#81cef1", "green": "#2d5c1b"
    });
    let (semantic, _) = semantic_palette(&iris).expect("semantic palette");
    let seed = swatch_from_palette(&semantic).into_iter().next().expect("dominant colour");
    for style in ["natural", "pastel", "muted", "vibrant"] {
        let tmp = tempfile::tempdir().unwrap();
        let config = styled_config(tmp.path(), style);
        let harmonised = styled_palette(&config, &seed, true).expect("harmonised palette");
        let doc = crate::material::document_with(&seed, true, &config.theme().scheme())
            .expect("scheme document");
        let applied =
            write_picker_palette_value(&config, &palette_from_scheme(&doc).expect("roles"));
        assert_eq!(harmonised, applied, "{style}");
        let raw = crate::style::restyle(&semantic, &config.theme().style());
        assert_ne!(harmonised, raw, "{style}");
    }
}

#[test]
fn preview_follows_style() {
    let tmp = tempfile::tempdir().unwrap();
    let natural = styled_palette(&styled_config(tmp.path(), "natural"), "#687538", true).unwrap();
    let pastel = styled_palette(&styled_config(tmp.path(), "pastel"), "#687538", true).unwrap();
    assert_ne!(natural, pastel);
}

#[test]
fn m3_palette_layer_separation() {
    for dark in [true, false] {
        let doc = crate::material::document("#f06e44", dark).unwrap();
        let pal = palette_from_scheme(&doc).expect("core roles present");
        let get = |key: &str| pal[key].as_str().unwrap().to_string();
        for role in ["surfaceVariant", "surfaceContainer", "background"] {
            assert_ne!(get(role), get("surface"), "dark={dark}: {role}");
        }
        let step = contrast_of(&get("surface"), &get("surfaceContainer"));
        assert!(step >= 1.20, "dark={dark}: {step:.2}");
        assert!(contrast_of(&get("surfaceText"), &get("surface")) >= 4.5, "dark={dark}");
    }
}

#[test]
fn preview_palettes_cover_schemes() {
    let config = Config::from_root(serde_json::json!({
        "theme": {
            "policy": "fixed",
            "staticTheme": "nord",
            "style": "natural",
            "mode": "dark"
        }
    }));
    let palettes = preview_palettes(&config, "/unused-for-static-theme.png");
    assert_eq!(palettes.len(), crate::material::SCHEMES.len());
    assert_eq!(
        palettes.iter().map(|(scheme, _)| scheme.as_str()).collect::<Vec<_>>(),
        crate::material::SCHEMES
    );
    assert!(palettes.iter().all(|(_, palette)| swatch_from_palette(palette).len() == 6));
}
