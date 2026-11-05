use super::*;

#[test]
fn red_reference() {
    let red = to_oklch(Rgb(255, 0, 0));
    assert!((red.light - 0.628).abs() < 1e-3, "light {}", red.light);
    assert!((red.chroma - 0.258).abs() < 1e-3, "chroma {}", red.chroma);
    assert!((red.hue - 29.23).abs() < 0.1, "hue {}", red.hue);
}

#[test]
fn round_trips() {
    for color in
        [Rgb(0, 0, 0), Rgb(255, 255, 255), Rgb(18, 52, 86), Rgb(247, 93, 90), Rgb(138, 184, 212)]
    {
        assert_eq!(from_oklch(to_oklch(color)), color);
    }
}

#[test]
fn clips_to_gamut() {
    let vivid = oklch(0.7, 0.4, 145.0);
    let back = to_oklch(vivid);
    assert!((back.light - 0.7).abs() < 0.02);
    assert!(back.chroma < 0.4);
}

#[test]
fn hue_helpers() {
    assert_eq!(hue_distance(350.0, 10.0), 20.0);
    assert_eq!(hue_offset(350.0, 10.0), 20.0);
    assert_eq!(hue_offset(10.0, 350.0), -20.0);
    assert!((mix_hue(350.0, 30.0, 0.5) - 10.0).abs() < 1e-4);
}
