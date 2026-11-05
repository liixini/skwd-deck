use serde_json::Value;

use super::*;
use crate::studio::analysis::Analysis;
use crate::studio::oklch::to_oklab;
use crate::studio::palette::Role;

const GOLDEN: &str = include_str!("golden.json");
const GOLDEN_GREY: &str = include_str!("golden_grey.json");
const SHAPES: &str = include_str!("shapes_golden.json");
const TOLERANCE: f32 = 0.03;

fn scene() -> Analysis {
    let (width, height) = (48, 30);
    let mut rgba = Vec::with_capacity(width * height * 4);
    for row in 0..height {
        for col in 0..width {
            let color = if (18..30).contains(&col) && (10..19).contains(&row) {
                [210, 50, 45]
            } else if row >= 24 {
                [40, 140, 150]
            } else {
                [20, 24, 40]
            };
            rgba.extend_from_slice(&[color[0], color[1], color[2], u8::MAX]);
        }
    }
    Analysis::new(&rgba, width, height).expect("scene")
}

fn grey_scene() -> Analysis {
    let (width, height) = (48, 30);
    let mut rgba = Vec::with_capacity(width * height * 4);
    for row in 0..height {
        for col in 0..width {
            let ramp = u8::try_from(col).expect("ramp");
            let color = if (row, col) == (15, 24) {
                [200, 70, 60]
            } else {
                [90 + ramp, 90 + ramp, 92 + ramp]
            };
            rgba.extend_from_slice(&[color[0], color[1], color[2], u8::MAX]);
        }
    }
    Analysis::new(&rgba, width, height).expect("grey scene")
}

fn distance(a: Rgb, b: Rgb) -> f32 {
    let (left, right) = (to_oklab(a), to_oklab(b));
    left.iter().zip(right).map(|(x, y)| (x - y).powi(2)).sum::<f32>().sqrt()
}

fn check(golden: &str, analysis: &Analysis) {
    let golden: Value = serde_json::from_str(golden).expect("golden");
    for method in Method::ALL {
        for (mode, dark) in [("dark", true), ("light", false)] {
            let expected = StudioPalette::from_wire(&golden[format!("{}-{mode}", method.key())])
                .expect("golden palette");
            let made = method.make(analysis, dark).palette;
            for role in Role::ALL {
                let gap = distance(made.get(role), expected.get(role));
                assert!(gap < TOLERANCE, "{} {mode} {}: {gap}", method.key(), role.key());
            }
            for (slot, (got, want)) in made.ansi.iter().zip(expected.ansi).enumerate() {
                assert!(distance(*got, want) < TOLERANCE, "{} {mode} ansi {slot}", method.key());
            }
        }
    }
}

#[test]
fn matches_prototype() {
    check(GOLDEN, &scene());
}

#[test]
fn grey_matches_prototype() {
    let analysis = grey_scene();
    assert!(analysis.grey);
    check(GOLDEN_GREY, &analysis);
}

#[test]
fn shapes_match_prototype() {
    let golden: Value = serde_json::from_str(SHAPES).expect("shapes");
    let pairs = |key: &str| -> Vec<[f32; 2]> {
        golden[key]
            .as_array()
            .expect("pairs")
            .iter()
            .map(|pair| [pair[0].as_f64().expect("x") as f32, pair[1].as_f64().expect("y") as f32])
            .collect()
    };
    let close = |got: &[[f32; 2]], want: &[[f32; 2]]| {
        got.len() == want.len()
            && got
                .iter()
                .zip(want)
                .all(|(a, b)| (a[0] - b[0]).abs() < 0.5 && (a[1] - b[1]).abs() < 0.01)
    };
    let Shape::Mask(mask) = Method::Gamut.make(&scene(), true).shape else {
        panic!("gamut mask");
    };
    assert!(close(&mask, &pairs("mask")), "{mask:?}");
    let Shape::Sectors(sectors) = Method::Templates.make(&scene(), true).shape else {
        panic!("template sectors");
    };
    assert!(close(&sectors, &pairs("sectors")), "{sectors:?}");
}

#[test]
fn families_split() {
    let theory = Method::ALL.iter().filter(|method| method.family() == Family::Theory).count();
    assert_eq!(theory, 7);
    assert_eq!(Method::ALL.len() - theory, 5);
}

#[test]
fn subject_marks_patch() {
    let mark = Method::Subject.make(&scene(), true).mark.expect("mark");
    assert!((mark[0] - 0.5).abs() < 0.1 && (mark[1] - 0.48).abs() < 0.1, "{mark:?}");
}

#[test]
fn wheel_marks_hues() {
    let bins = wheel(&scene());
    assert!(bins[2] > 0.0, "red bin");
    assert_eq!(bins.iter().copied().fold(0.0, f32::max), 1.0);
    assert_eq!(bins.iter().filter(|bin| **bin > 0.0).count(), 2);
}

#[test]
fn keys_round_trip() {
    for method in Method::ALL {
        assert_eq!(Method::from_key(method.key()), Some(method));
    }
}
