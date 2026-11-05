use super::*;
use crate::semantic::contrast;
use crate::studio::polish::READABLE_CONTRAST;

#[test]
fn presets_become_readable() {
    for (key, _) in crate::PRESETS {
        let theme = crate::preset(key).expect("preset");
        let palette = from_theme(&theme);
        assert!(
            contrast(palette.text, palette.raised) >= crate::studio::polish::TEXT_CONTRAST,
            "{key}"
        );
        for slot in &palette.ansi[1..ANSI_SLOTS / 2 - 1] {
            assert!(contrast(*slot, palette.base) >= READABLE_CONTRAST, "{key}");
        }
    }
}

#[test]
fn second_is_softer() {
    let theme = crate::preset("gruvbox").expect("gruvbox");
    let palette = from_theme(&theme);
    assert!(to_oklch(palette.second).chroma < to_oklch(palette.accent).chroma);
}

#[cfg(feature = "material")]
#[test]
fn material_maps_roles() {
    let document = crate::material::document_with("#d77e52", true, "content").expect("document");
    let palette = from_material(&document, true).expect("palette");
    assert_eq!(Some(palette.second.hex()), crate::material::role(&document, "secondary", "dark"));
}
