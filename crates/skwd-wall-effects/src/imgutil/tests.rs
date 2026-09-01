#![cfg(test)]

use super::*;

#[test]
fn parse_hex_forms() {
    assert_eq!(parse_hex("#ff8800"), (255, 136, 0));
    assert_eq!(parse_hex("00ff00"), (0, 255, 0));
    assert_eq!(parse_hex("#80ff8800"), (255, 136, 0));
    assert_eq!(parse_hex("nope"), (0, 0, 0));
}

#[test]
fn hsl_round_trip() {
    for &(r, g, b) in &[(200u8, 50u8, 100u8), (10, 220, 30), (128, 128, 128)] {
        let (h, s, l) = rgb_to_hsl(r, g, b);
        let (r2, g2, b2) = hsl_to_rgb(h, s, l);
        assert!((i32::from(r) - i32::from(r2)).abs() <= 2);
        assert!((i32::from(g) - i32::from(g2)).abs() <= 2);
        assert!((i32::from(b) - i32::from(b2)).abs() <= 2);
    }
}

#[test]
fn gradient_endpoints() {
    let stops = [(0, 0, 0), (255, 255, 255)];
    assert_eq!(gradient_sample(&stops, 0.0), (0, 0, 0));
    assert_eq!(gradient_sample(&stops, 1.0), (255, 255, 255));
    let mid = gradient_sample(&stops, 0.5);
    assert!(mid.0 > 120 && mid.0 < 135);
}
