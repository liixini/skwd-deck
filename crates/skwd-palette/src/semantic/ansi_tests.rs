use std::collections::HashSet;

use super::*;
use crate::semantic::contrast;

fn scene() -> Vec<u8> {
    let mut output = Vec::new();
    for (color, count) in [
        (Rgb(28, 36, 44), 900),
        (Rgb(52, 74, 92), 500),
        (Rgb(196, 92, 60), 260),
        (Rgb(212, 206, 190), 180),
    ] {
        for _ in 0..count {
            output.extend_from_slice(&[color.0, color.1, color.2, 255]);
        }
    }
    output
}

#[test]
fn ansi16_pywal_slots() {
    for dark in [true, false] {
        let colors = ansi16(&scene(), 0, 0, dark).unwrap();
        assert_eq!(colors.len(), 16);
        assert!(contrast(colors[0], colors[7]) >= 4.5);
        let distinct: HashSet<(u8, u8, u8)> =
            colors.iter().map(|color| (color.0, color.1, color.2)).collect();
        assert!(distinct.len() >= 8);
    }
}

#[test]
fn wallust_variants_distinct() {
    let mut seen = HashSet::new();
    for variant in ANSI_VARIANTS {
        let colors = ansi16_variant(&scene(), 0, 0, true, variant).unwrap();
        assert_eq!(colors.len(), 16);
        assert!(contrast(colors[0], colors[7]) >= 4.5);
        seen.insert(colors.iter().map(|color| (color.0, color.1, color.2)).collect::<Vec<_>>());
    }
    assert_eq!(seen.len(), ANSI_VARIANTS.len());
}
