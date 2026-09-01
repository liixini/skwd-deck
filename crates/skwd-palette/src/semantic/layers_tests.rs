use super::*;

#[test]
fn ui_layers_distinct() {
    let semantic = Semantic {
        bg: Rgb(28, 36, 44),
        surface: Rgb(42, 50, 58),
        fg: Rgb(220, 225, 230),
        dim: Rgb(120, 130, 140),
        accent: Rgb(196, 92, 60),
    };
    let palette = derive_ui_palette(&semantic);
    assert_eq!(palette.surface, semantic.bg);
    assert_eq!(palette.surface_text, semantic.fg);
    assert_eq!(palette.primary, semantic.accent);
    assert_eq!(palette.outline, semantic.dim);
    assert_ne!(palette.surface_variant, palette.surface);
    assert_ne!(palette.surface_container, palette.surface);
    assert_ne!(palette.background, palette.surface);
    assert!(contrast(palette.surface, palette.surface_container) >= 1.45);
}

#[test]
fn layers_at_luminance_extremes() {
    for semantic in [
        Semantic {
            bg: Rgb(250, 250, 238),
            surface: Rgb(240, 240, 228),
            fg: Rgb(30, 34, 24),
            dim: Rgb(110, 115, 100),
            accent: Rgb(90, 120, 70),
        },
        Semantic {
            bg: Rgb(8, 9, 10),
            surface: Rgb(18, 19, 20),
            fg: Rgb(230, 232, 228),
            dim: Rgb(115, 120, 110),
            accent: Rgb(120, 140, 90),
        },
    ] {
        let palette = derive_ui_palette(&semantic);
        assert_ne!(palette.surface_variant, palette.surface);
        assert_ne!(palette.surface_container, palette.surface);
        assert_ne!(palette.background, palette.surface);
    }
}
