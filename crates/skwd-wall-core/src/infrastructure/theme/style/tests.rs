#![cfg(test)]

use super::{STYLES, is_dark_palette, restyle};
use serde_json::json;

fn dark_palette() -> serde_json::Value {
    json!({
        "primary": "#89b4fa", "primaryText": "#11141c", "tertiary": "#cba6f7",
        "surface": "#1e2030", "surfaceText": "#e6e9f0", "surfaceVariant": "#2a2d40",
        "surfaceContainer": "#242739", "background": "#181926", "outline": "#6c7086"
    })
}

fn light_palette() -> serde_json::Value {
    json!({
        "primary": "#3f5683", "primaryText": "#ffffff", "tertiary": "#6c3f83",
        "surface": "#dceaf6", "surfaceText": "#161a1f", "surfaceVariant": "#cfe0ef",
        "surfaceContainer": "#c2d6e8", "background": "#dceaf6", "outline": "#7d8f9e"
    })
}

fn sat(hex: &str) -> f32 {
    let digits = hex.trim_start_matches('#');
    let chan = |idx: usize| u8::from_str_radix(&digits[idx..idx + 2], 16).unwrap();
    let col = skwd_palette::Rgb(chan(0), chan(2), chan(4));
    skwd_palette::to_hsl(col).1
}

#[test]
fn polarity_from_palette() {
    assert!(is_dark_palette(&dark_palette()));
    assert!(!is_dark_palette(&light_palette()));
}

#[test]
fn natural_passthrough() {
    for palette in [dark_palette(), light_palette()] {
        assert_eq!(restyle(&palette, "natural"), palette);
        assert_eq!(restyle(&palette, "anything-unknown"), palette);
    }
}

#[test]
fn muted_calms_vibrant_pushes() {
    for palette in [dark_palette(), light_palette()] {
        let base = sat(palette["primary"].as_str().unwrap());
        let muted = sat(restyle(&palette, "muted")["primary"].as_str().unwrap());
        let vivid = sat(restyle(&palette, "vibrant")["primary"].as_str().unwrap());
        assert!(muted < base, "{muted} vs {base}");
        assert!(vivid >= base, "{vivid} vs {base}");
    }
    let flat = json!({
        "primary": "#687538", "primaryText": "#ffffff", "tertiary": "#5a7a4a",
        "surface": "#cfdad7", "surfaceText": "#1a1e1d", "surfaceVariant": "#c6d3cf",
        "surfaceContainer": "#bdccc8", "background": "#cfdad7", "outline": "#8fa09b"
    });
    let base = sat(flat["surface"].as_str().unwrap());
    let vivid = sat(restyle(&flat, "vibrant")["surface"].as_str().unwrap());
    assert!(vivid > base * 2.0, "{base} -> {vivid}");
}

#[test]
fn pastel_saturation_target() {
    let target = 0.45f32;
    for palette in [dark_palette(), light_palette()] {
        let base = sat(palette["surface"].as_str().unwrap());
        let out = sat(restyle(&palette, "pastel")["surface"].as_str().unwrap());
        assert!((out - target).abs() < (base - target).abs(), "{target}: {base} -> {out}");
    }
}

#[test]
fn pastel_rescues_washed_out() {
    let washed = json!({
        "primary": "#687538", "primaryText": "#ffffff", "tertiary": "#5a7a4a",
        "surface": "#cfdad7", "surfaceText": "#1a1e1d", "surfaceVariant": "#c6d3cf",
        "surfaceContainer": "#bdccc8", "background": "#cfdad7", "outline": "#8fa09b"
    });
    let out = restyle(&washed, "pastel");
    let before = sat(washed["surface"].as_str().unwrap());
    let after = sat(out["surface"].as_str().unwrap());
    assert!(after > before * 2.0, "{before} -> {after}");
    assert!(super::readable(&out));
}

#[test]
fn text_roles_unchanged() {
    for palette in [dark_palette(), light_palette()] {
        for style in STYLES {
            let out = restyle(&palette, style);
            assert_eq!(out["surfaceText"], palette["surfaceText"], "{style}");
            assert_eq!(out["primaryText"], palette["primaryText"], "{style}");
        }
    }
}

#[test]
fn every_style_readable() {
    for palette in [dark_palette(), light_palette()] {
        for style in STYLES {
            let out = restyle(&palette, style);
            assert!(super::readable(&out), "{style} {palette}");
        }
    }
}

#[test]
fn pastel_lifts_light_only() {
    let lightness = |hex: &str| {
        let digits = hex.trim_start_matches('#');
        let chan = |idx: usize| u8::from_str_radix(&digits[idx..idx + 2], 16).unwrap();
        skwd_palette::to_hsl(skwd_palette::Rgb(chan(0), chan(2), chan(4))).2
    };
    let base = lightness(light_palette()["surface"].as_str().unwrap());
    let out = lightness(restyle(&light_palette(), "pastel")["surface"].as_str().unwrap());
    assert!((out - 0.86).abs() < (base - 0.86).abs(), "{base} -> {out}");
    let dark = restyle(&dark_palette(), "pastel");
    assert!(lightness(dark["surface"].as_str().unwrap()) < 0.4);
}

#[test]
fn styles_preserve_layer_separation() {
    let ramp = json!({
        "primary": "#4e6728", "primaryText": "#ffffff", "tertiary": "#286728",
        "surface": "#f2f2db", "surfaceText": "#1a1c16", "surfaceVariant": "#e4e6c8",
        "surfaceContainer": "#d6d9b6", "background": "#fafaee", "outline": "#8fa09b"
    });
    let gap = |palette: &serde_json::Value| {
        let light = |key: &str| {
            let digits = palette[key].as_str().unwrap().trim_start_matches('#').to_string();
            let chan = |idx: usize| u8::from_str_radix(&digits[idx..idx + 2], 16).unwrap();
            skwd_palette::to_hsl(skwd_palette::Rgb(chan(0), chan(2), chan(4))).2
        };
        (light("surface") - light("surfaceContainer")).abs()
    };
    let base = gap(&ramp);
    for style in STYLES {
        let out = restyle(&ramp, style);
        assert!(gap(&out) >= base * 0.7, "{style}: {base:.3} -> {:.3}", gap(&out));
    }
}
