#![cfg(test)]

use super::{document, parse_seed, role};

const SEED: &str = "#f06e44";

#[test]
fn matches_matugen_reference() {
    let doc = document(SEED, true).expect("document for a valid seed");
    for (name, dark, light) in [
        ("primary", "#ffb59e", "#8f4c36"),
        ("on_primary", "#561f0e", "#ffffff"),
        ("surface", "#1a110f", "#fff8f6"),
        ("surface_container_high", "#322825", "#f7e4df"),
        ("outline", "#a08c87", "#85736e"),
        ("tertiary", "#d7c68d", "#6b5e2f"),
        ("error", "#ffb4ab", "#ba1a1a"),
    ] {
        assert_eq!(role(&doc, name, "dark").as_deref(), Some(dark), "{name} dark");
        assert_eq!(role(&doc, name, "light").as_deref(), Some(light), "{name} light");
    }
}

#[test]
fn document_shape() {
    let doc = document(SEED, true).unwrap();
    assert_eq!(doc["mode"], "dark");
    assert_eq!(doc["is_dark_mode"], true);
    let colors = doc["colors"].as_object().unwrap();
    assert!(colors.len() >= 50, "got {}", colors.len());
    for required in ["primary", "on_primary", "surface", "on_surface", "outline", "source_color"] {
        assert!(colors.contains_key(required), "{required}");
    }
    assert_eq!(role(&doc, "source_color", "default").as_deref(), Some(SEED));
}

#[test]
fn default_follows_mode() {
    let dark = document(SEED, true).unwrap();
    assert_eq!(role(&dark, "surface", "default"), role(&dark, "surface", "dark"));
    let light = document(SEED, false).unwrap();
    assert_eq!(role(&light, "surface", "default"), role(&light, "surface", "light"));
    assert_eq!(light["mode"], "light");
    assert_eq!(role(&light, "surface", "dark"), role(&dark, "surface", "dark"));
}

#[test]
fn seed_parsing_strict() {
    assert_eq!(parse_seed("#F06E44").as_deref(), Some("#f06e44"));
    assert_eq!(parse_seed("f06e44").as_deref(), Some("#f06e44"));
    assert_eq!(parse_seed("  #f06e44  ").as_deref(), Some("#f06e44"));
    for bad in ["#aéabc", "éééééé", "", "#12345", "#1234567", "#gggggg", "nonsense"] {
        assert_eq!(parse_seed(bad), None, "{bad}");
        assert!(document(bad, true).is_none(), "{bad}");
    }
}

#[test]
fn every_scheme_full_document() {
    use super::{SCHEMES, document_with};
    let mut seen = std::collections::HashSet::new();
    for scheme in SCHEMES {
        let doc = document_with(SEED, true, scheme).unwrap_or_else(|| panic!("{scheme} failed"));
        assert!(doc["colors"].as_object().unwrap().len() >= 50, "{scheme} short document");
        seen.insert(role(&doc, "primary", "dark").unwrap());
    }
    assert!(seen.len() > 1, "{seen:?}");
}

#[test]
fn unknown_scheme_tonal_spot() {
    use super::{document, document_with};
    assert_eq!(document_with(SEED, true, "nonsense"), document(SEED, true));
    assert_eq!(document_with(SEED, true, "tonal-spot"), document(SEED, true));
}

#[test]
fn document_base16_complete() {
    use super::BASE16_KEYS;
    for dark in [true, false] {
        let doc = document(SEED, dark).unwrap();
        let b16 = doc["base16"].as_object().expect("base16 section present");
        assert_eq!(b16.len(), 16);
        for key in BASE16_KEYS {
            let hex = b16[key].as_str().unwrap_or_else(|| panic!("{key} missing"));
            assert!(hex.len() == 7 && hex.starts_with('#'), "dark={dark}: {key}={hex}");
        }
        let bg = b16["base00"].as_str().unwrap();
        let fg = b16["base05"].as_str().unwrap();
        assert_ne!(bg, fg, "dark={dark}");
        let accents: std::collections::HashSet<&str> = ["base08", "base09", "base0B", "base0D"]
            .iter()
            .map(|key| b16[*key].as_str().unwrap())
            .collect();
        assert!(accents.len() >= 3, "dark={dark}: {accents:?}");
    }
}
