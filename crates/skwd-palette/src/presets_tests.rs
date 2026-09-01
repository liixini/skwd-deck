use super::*;

#[test]
fn all_presets_resolve() {
    for (key, _) in PRESETS {
        let palette = preset(key).unwrap_or_else(|| panic!("preset {key} missing"));
        for color in [
            palette.primary,
            palette.surface,
            palette.surface_variant,
            palette.background,
            palette.outline,
            palette.tertiary,
        ] {
            assert_eq!(color.hex().len(), 7, "{key}");
        }
    }
    assert!(preset("nope").is_none());
}

#[test]
fn preset_text_accent_contrast() {
    let saturation = |color: Rgb| {
        let max = color.0.max(color.1).max(color.2);
        let min = color.0.min(color.1).min(color.2);
        if max == 0 { 0.0 } else { f32::from(max - min) / f32::from(max) }
    };
    for (key, _) in PRESETS {
        let palette = preset(key).unwrap();
        assert!((palette.on_surface.lum() - palette.surface.lum()).abs() / 255.0 > 0.3, "{key}");
        assert!((palette.primary.lum() - palette.on_primary.lum()).abs() / 255.0 > 0.2);
        assert!(saturation(palette.primary) > 0.15);
        assert_ne!(palette.primary, palette.tertiary);
    }
}

#[test]
fn nord_signature_is_stable() {
    let value = preset("nord").unwrap().to_value();
    assert_eq!(value["background"], "#2e3440");
    assert_eq!(value["primary"], "#88c0d0");
}
