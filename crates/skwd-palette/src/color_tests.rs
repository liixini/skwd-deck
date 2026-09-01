use super::*;

#[test]
fn rgb_hex_lowercase() {
    assert_eq!(Rgb(255, 180, 171).hex(), "#ffb4ab");
    assert_eq!(Rgb(0, 0, 0).hex(), "#000000");
}

#[test]
fn hsl_round_trip() {
    for color in [Rgb(200, 40, 60), Rgb(30, 120, 200), Rgb(120, 200, 40)] {
        let (hue, saturation, light) = to_hsl(color);
        let back = from_hsl(hue, saturation, light);
        assert!((back.0 as i32 - color.0 as i32).abs() <= 3);
        assert!((back.1 as i32 - color.1 as i32).abs() <= 3);
        assert!((back.2 as i32 - color.2 as i32).abs() <= 3);
    }
}
