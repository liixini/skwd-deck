use serde_json::Value;

use super::*;
use crate::studio::palette::Role;

const GOLDEN: &str = include_str!("area_golden.json");
const TOLERANCE: f32 = 0.03;

fn distance(a: Rgb, b: Rgb) -> f32 {
    let (left, right) = (to_oklab(a), to_oklab(b));
    left.iter().zip(right).map(|(x, y)| (x - y).powi(2)).sum::<f32>().sqrt()
}

fn scene(value: &Value) -> Vec<Patch> {
    value
        .as_array()
        .expect("scene")
        .iter()
        .map(|patch| {
            let number = |key: &str| patch[key].as_f64().expect("number") as f32;
            Patch {
                lch: Oklch { light: number("L"), chroma: number("C"), hue: number("H") },
                share: number("share"),
            }
        })
        .collect()
}

#[test]
fn matches_design() {
    let golden: Value = serde_json::from_str(GOLDEN).expect("golden");
    for name in ["vivid", "grey", "lone"] {
        let found = scene(&golden["scenes"][name]);
        for (mode, dark) in [("dark", true), ("light", false)] {
            let expected = StudioPalette::from_wire(&golden[format!("{name}-{mode}")])
                .expect("golden palette");
            let made = palette(&found, dark).expect("palette");
            for role in Role::ALL {
                let gap = distance(made.get(role), expected.get(role));
                assert!(gap < TOLERANCE, "{name} {mode} {}: {gap}", role.key());
            }
            for (slot, (got, want)) in made.ansi.iter().zip(expected.ansi).enumerate() {
                assert!(distance(*got, want) < TOLERANCE, "{name} {mode} ansi {slot}");
            }
        }
    }
}

#[test]
fn box_reads_colour() {
    let (width, height) = (40, 20);
    let mut rgba = Vec::with_capacity(width * height * RGBA);
    for _row in 0..height {
        for col in 0..width {
            let color = if col < width / 2 { [200, 40, 40] } else { [30, 60, 200] };
            rgba.extend_from_slice(&[color[0], color[1], color[2], u8::MAX]);
        }
    }
    let right = Area { left: 0.5, top: 0.0, right: 1.0, bottom: 1.0 };
    let found = patches(&rgba, width, height, right);
    let accent = seed(&found).expect("seed");
    assert!((accent.lch.hue - 264.0).abs() < 10.0, "{:?}", accent.lch);
    assert!(wheel(&found).iter().filter(|bin| **bin > 0.0).count() == 1);
}

#[test]
fn edges_round_trip() {
    let area = Area::from_edges(&[0.1, 0.2, 0.6, 0.9]).expect("area");
    assert_eq!(Area::from_edges(&area.edges()), Some(area));
    assert!(Area::from_edges(&[0.6, 0.2, 0.1, 0.9]).is_none());
    assert!(Area::from_edges(&[0.1, 0.2]).is_none());
}
