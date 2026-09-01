use super::*;
use crate::seed::{ACHROMATIC_CHROMA, chroma_of};

fn image(colors: &[(Rgb, usize)]) -> Vec<u8> {
    let mut output = Vec::new();
    for (color, count) in colors {
        for _ in 0..*count {
            output.extend_from_slice(&[color.0, color.1, color.2, 255]);
        }
    }
    output
}

fn scene() -> Vec<u8> {
    image(&[
        (Rgb(28, 36, 44), 900),
        (Rgb(52, 74, 92), 500),
        (Rgb(196, 92, 60), 260),
        (Rgb(212, 206, 190), 180),
    ])
}

#[test]
fn scheme_role_ordering() {
    let dark = semantic(&scene(), 0, 0, true).unwrap();
    let light = |color: Rgb| crate::to_hsl(color).2;
    assert!(light(dark.bg) < light(dark.surface));
    assert!(light(dark.surface) < light(dark.dim));
    assert!(light(dark.dim) < light(dark.fg));

    let light_scheme = semantic(&scene(), 0, 0, false).unwrap();
    assert!(light(light_scheme.bg) > light(light_scheme.surface));
    assert!(light(light_scheme.fg) < light(light_scheme.dim));
}

#[test]
fn text_accent_contrast() {
    for dark in [true, false] {
        let semantic = semantic(&scene(), 0, 0, dark).unwrap();
        assert!(contrast(semantic.fg, semantic.bg) >= 4.5);
        assert!(contrast(semantic.accent, semantic.bg) >= 2.9);
    }
}

#[test]
fn greyscale_neutral_roles() {
    let grey =
        image(&[(Rgb(24, 24, 24), 700), (Rgb(120, 120, 120), 400), (Rgb(220, 220, 220), 200)]);
    let semantic = semantic(&grey, 0, 0, true).unwrap();
    for role in [semantic.bg, semantic.surface, semantic.fg, semantic.dim] {
        assert!(chroma_of(role) < ACHROMATIC_CHROMA);
    }
    assert!(chroma_of(semantic.accent) >= ACHROMATIC_CHROMA);
}

#[test]
fn semantic_deterministic() {
    assert_eq!(semantic(&scene(), 0, 0, true), semantic(&scene(), 0, 0, true));
    assert!(semantic(&[], 0, 0, true).is_none());
    assert!(semantic(&[9, 9, 9], 0, 0, true).is_none());
}

#[test]
fn pure_black_and_white() {
    let black = semantic(&image(&[(Rgb(0, 0, 0), 400)]), 0, 0, true).unwrap();
    assert!(crate::to_hsl(black.bg).2 < 0.05);
    assert!(contrast(black.fg, black.bg) >= 4.5);

    let white = semantic(&image(&[(Rgb(255, 255, 255), 400)]), 0, 0, false).unwrap();
    assert!(crate::to_hsl(white.bg).2 > 0.95);
    assert!(contrast(white.fg, white.bg) >= 4.5);
}

#[test]
fn near_grey_accent() {
    let near_grey = image(&[
        (Rgb(34, 34, 32), 700),
        (Rgb(120, 118, 116), 400),
        (Rgb(210, 208, 206), 200),
        (Rgb(96, 92, 78), 60),
    ]);
    for dark in [true, false] {
        let semantic = semantic(&near_grey, 0, 0, dark).unwrap();
        assert!(chroma_of(semantic.accent) >= ACHROMATIC_CHROMA);
        assert!(contrast(semantic.accent, semantic.bg) >= 2.5);
    }
}
