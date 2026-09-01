#![cfg(test)]

use super::*;
use serde_json::json;

fn nordish() -> Value {
    json!({
        "primary": "#88c0d0", "primaryText": "#2e3440", "tertiary": "#a3be8c",
        "surface": "#2e3440", "surfaceText": "#d8dee9", "surfaceVariant": "#3b4252",
        "surfaceContainer": "#434c5e", "background": "#242933", "outline": "#4c566a"
    })
}

const CENSUS: [&str; 47] = [
    "ansi_blue",
    "ansi_blue_bright",
    "ansi_cyan",
    "ansi_cyan_bright",
    "ansi_green",
    "ansi_green_bright",
    "ansi_magenta",
    "ansi_magenta_bright",
    "ansi_red",
    "ansi_red_bright",
    "ansi_yellow",
    "ansi_yellow_bright",
    "background",
    "error",
    "error_container",
    "inverse_on_surface",
    "inverse_primary",
    "inverse_surface",
    "on_background",
    "on_error",
    "on_error_container",
    "on_primary",
    "on_primary_container",
    "on_secondary",
    "on_secondary_container",
    "on_surface",
    "on_surface_variant",
    "on_tertiary",
    "on_tertiary_container",
    "outline",
    "outline_variant",
    "primary",
    "primary_container",
    "secondary",
    "secondary_container",
    "shadow",
    "surface",
    "surface_bright",
    "surface_container",
    "surface_container_high",
    "surface_container_highest",
    "surface_container_low",
    "surface_container_lowest",
    "surface_dim",
    "surface_variant",
    "tertiary",
    "tertiary_container",
];

#[test]
fn material_map_census() {
    let map = material_map(&nordish(), true);
    for token in CENSUS {
        assert!(map.contains_key(token), "{token}");
    }
    assert_eq!(map["primary"], Rgb(0x88, 0xc0, 0xd0));
    assert_eq!(map["on_surface"], Rgb(0xd8, 0xde, 0xe9));
    assert_eq!(map["surface_container_lowest"], map["background"]);
    assert_ne!(map["error"], map["primary"]);
}

#[test]
fn ansi_ramp_distinct() {
    let grey = json!({ "primary": "#808080", "surface": "#202020" });
    let slots = ["ansi_red", "ansi_yellow", "ansi_green", "ansi_cyan", "ansi_blue", "ansi_magenta"];
    for (palette, dark) in [(nordish(), true), (nordish(), false), (grey, true)] {
        let map = material_map(&palette, dark);
        for first in 0..slots.len() {
            for second in first + 1..slots.len() {
                assert_ne!(
                    map[slots[first]], map[slots[second]],
                    "{} vs {}",
                    slots[first], slots[second]
                );
            }
        }
        for slot in slots {
            let bright = format!("{slot}_bright");
            assert_ne!(map[slot], map[bright.as_str()], "{slot}");
        }
    }
}

#[test]
fn render_formats() {
    let map = material_map(&nordish(), true);
    let out = render(
        "a={{colors.primary.default.hex}} b={{colors.primary.default.hex_stripped}} \
         r={{colors.primary.default.red}} g={{colors.primary.default.green}} \
         bl={{colors.primary.default.blue}}",
        &map,
    );
    assert_eq!(out, "a=#88c0d0 b=88c0d0 r=136 g=192 bl=208");
}

#[test]
fn render_bridge_full_set() {
    let tmp = tempfile::tempdir().unwrap();
    let tdir = tmp.path().join("templates");
    std::fs::create_dir_all(&tdir).unwrap();
    std::fs::write(
        tdir.join("bridge.json"),
        r#"{"primary":"{{colors.primary.default.hex}}","secondary":"{{colors.secondary.default.hex}}","primaryContainer":"{{colors.primary_container.default.hex}}","background":"{{colors.surface_container_lowest.default.hex}}","surfaceContainer":"{{colors.surface_container_highest.default.hex}}","error":"{{colors.error.default.hex}}"}"#,
    )
    .unwrap();
    let config = crate::config::Config::from_root(json!({
        "paths": { "cache": tmp.path().to_string_lossy(), "templates": tdir.to_string_lossy() },
        "integrations": [{ "name": "bridge", "output": "colors.json", "template": "bridge.json" }],
    }));
    let out = render_bridge(&config, &nordish(), true).expect("bridge integration rendered");
    let parsed: Value = serde_json::from_str(&out).expect("valid json bridge");
    for role in
        ["primary", "secondary", "primaryContainer", "background", "surfaceContainer", "error"]
    {
        let hex = parsed[role].as_str().unwrap_or("");
        assert!(hex.starts_with('#') && hex.len() == 7, "{role}: {hex:?}");
    }
    assert_ne!(parsed["secondary"], parsed["primary"]);
    assert_eq!(parsed["background"], "#242933");
    assert_eq!(parsed["surfaceContainer"], "#434c5e");
    assert_eq!(
        render_bridge(
            &crate::config::Config::from_root(
                json!({ "paths": { "cache": tmp.path().to_string_lossy() } })
            ),
            &nordish(),
            true
        ),
        None
    );
}

#[test]
fn render_integrations_filter() {
    let tmp = tempfile::tempdir().unwrap();
    let tdir = tmp.path().join("templates");
    std::fs::create_dir_all(&tdir).unwrap();
    std::fs::write(tdir.join("t.conf"), "primary={{colors.primary.default.hex}}").unwrap();
    let out_live = tmp.path().join("live.conf");
    let out_skip = tmp.path().join("skip.conf");
    let config = crate::config::Config::from_root(json!({
        "paths": { "cache": tmp.path().to_string_lossy(), "templates": tdir.to_string_lossy() },
        "integrations": [
            { "output": out_live.to_string_lossy(), "template": "t.conf" },
            { "output": out_skip.to_string_lossy(), "template": "t.conf", "livePreview": false },
        ],
    }));
    let written = render_integrations_where(&config, &nordish(), true, |integ| integ.live_preview);
    assert_eq!(written, 1);
    assert!(out_live.exists());
    assert!(!out_skip.exists());
}

#[test]
fn render_keeps_unknown() {
    let map = material_map(&nordish(), true);
    for weird in [
        "{{colors.moonbeam.default.hex}}",
        "{{colors.primary.default.hsl}}",
        "{{image}}",
        "{{colors.primary.default.hex | set_alpha: 0.5}}",
        "{{unclosed",
    ] {
        let input = format!("keep {weird} intact");
        let out = render(&input, &map);
        assert!(out.contains(weird), "{weird}");
    }
}

#[test]
fn render_rgb_triplets() {
    let map = material_map(&nordish(), true);
    let out = render(
        "{{colors.tertiary.default.red}},{{colors.tertiary.default.green}},{{colors.tertiary.default.blue}}",
        &map,
    );
    assert_eq!(out, "163,190,140");
}

#[test]
fn light_flips_error() {
    let dark = material_map(&nordish(), true);
    let light = material_map(&nordish(), false);
    assert_ne!(dark["error"], light["error"]);
    assert_eq!(light["on_error"], Rgb(0xff, 0xff, 0xff));
}

#[test]
fn parse_multibyte() {
    assert_eq!(parse("#aéabc"), None);
    assert_eq!(parse("éééééé"), None);
    assert_eq!(parse("#ff8800"), Some(Rgb(255, 136, 0)));
}

#[test]
fn render_doc_scheme_segment() {
    let doc = crate::material::document("#f06e44", true).unwrap();
    let out = render_doc(
        "d={{colors.primary.dark.hex}} l={{colors.primary.light.hex}} n={{colors.primary.default.hex}}",
        &doc,
    );
    assert_eq!(out, "d=#ffb59e l=#8f4c36 n=#ffb59e");
    let light = crate::material::document("#f06e44", false).unwrap();
    assert!(render_doc("{{colors.primary.default.hex}}", &light).contains("#8f4c36"));
}

#[test]
fn render_doc_extra_roles() {
    let doc = crate::material::document("#f06e44", true).unwrap();
    for role in ["surface_container_high", "on_primary_fixed_variant", "inverse_surface", "scrim"] {
        let out = render_doc(&format!("{{{{colors.{role}.dark.hex}}}}"), &doc);
        assert!(out.starts_with('#') && out.len() == 7, "{role}: {out}");
    }
}

#[test]
fn render_doc_formats_unknowns() {
    let doc = crate::material::document("#f06e44", true).unwrap();
    assert_eq!(render_doc("{{colors.primary.dark.hex_stripped}}", &doc), "ffb59e");
    assert_eq!(render_doc("{{colors.primary.dark.rgb}}", &doc), "rgb(255, 181, 158)");
    assert_eq!(render_doc("{{colors.nope.dark.hex}}", &doc), "{{colors.nope.dark.hex}}");
    assert_eq!(render_doc("{{ .Shell }}", &doc), "{{ .Shell }}");
}

#[test]
fn render_doc_base16_slots() {
    let doc = crate::material::document("#f06e44", true).unwrap();
    let out = render_doc("{{base16.base00.default.hex}} {{base16.base0D.hex}}", &doc);
    let parts: Vec<&str> = out.split_whitespace().collect();
    assert_eq!(parts.len(), 2, "{out}");
    for hex in parts {
        assert!(hex.starts_with('#') && hex.len() == 7, "{hex}");
    }
    assert_eq!(render_doc("{{base16.nope.hex}}", &doc), "{{base16.nope.hex}}");
}
