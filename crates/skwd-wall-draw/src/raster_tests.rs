use super::*;

fn covered(start: Point, end: Point, width: f32) -> Vec<(usize, f32, f32)> {
    let mut hits = Vec::new();
    stamp(start, end, width, [10, 10], |index, coverage, fraction| {
        hits.push((index, coverage, fraction));
    });
    hits
}

#[test]
fn covers_segment_axis() {
    let hits = covered([2.0, 5.0], [7.0, 5.0], 1.0);
    for x in 2..=7 {
        assert!(hits.iter().any(|&(index, coverage, _)| index == 50 + x && coverage == 1.0));
    }
}

#[test]
fn fraction_runs_along() {
    let hits = covered([2.0, 5.0], [6.0, 5.0], 1.0);
    let at = |x: usize| hits.iter().find(|hit| hit.0 == 50 + x).unwrap().2;
    assert_eq!(at(2), 0.0);
    assert_eq!(at(4), 0.5);
    assert_eq!(at(6), 1.0);
}

#[test]
fn clips_to_bounds() {
    let hits = covered([-5.0, -5.0], [20.0, 20.0], 3.0);
    assert!(hits.iter().all(|&(index, _, _)| index < 100));
    assert!(!hits.is_empty());
}

#[test]
fn measures_paths() {
    assert_eq!(length(&[[0.0, 0.0], [3.0, 4.0], [3.0, 10.0]]), 11.0);
    assert_eq!(centre(&[[0.0, 0.0], [4.0, 2.0]]), [2.0, 1.0]);
}
