use super::*;

#[test]
fn derive_light_direction() {
    let dark = derive(&[Rgb(40, 120, 220)], true);
    assert!(dark.background.lum() < dark.on_surface.lum());
    assert!(dark.surface.lum() < dark.on_surface.lum());

    let light = derive(&[Rgb(40, 120, 220)], false);
    assert!(light.background.lum() > light.on_surface.lum());
}

#[test]
fn grey_input_gets_accent() {
    for dark in [true, false] {
        for dominant in [&[][..], &[Rgb(128, 128, 128), Rgb(30, 30, 30)][..]] {
            let palette = derive(dominant, dark);
            let (hue, saturation, _) = to_hsl(palette.primary);
            assert!(saturation >= 0.35);
            if dominant.is_empty() {
                assert!((hue - 265.0).abs() < 20.0);
            }
        }
    }
}

#[test]
fn role_luminance_separation() {
    for index in 0..20 {
        let hue = index as f32 * 18.0;
        let accent = from_hsl(hue, 0.8, 0.5);
        for dark in [true, false] {
            let palette = derive(&[accent], dark);
            assert!((palette.primary.lum() - palette.on_primary.lum()).abs() > 60.0);
            assert!((palette.surface.lum() - palette.on_surface.lum()).abs() > 100.0);
            assert!((palette.background.lum() - palette.on_surface.lum()).abs() > 100.0);
        }
    }
}
