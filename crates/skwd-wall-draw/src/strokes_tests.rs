use super::*;

fn canvas(width: usize, height: usize, lines: &[(Point, Point, f32)]) -> Plane {
    let mut plane = Plane::filled(width, height, 0.0);
    for &(start, end, width) in lines {
        raster::stamp(start, end, width, [plane.width, plane.height], |index, coverage, _| {
            plane.data[index] = plane.data[index].max(coverage);
        });
    }
    plane
}

#[test]
fn thins_to_one_pixel() {
    let size = [40, 20];
    let thick = canvas(size[0], size[1], &[([5.0, 10.0], [34.0, 10.0], 5.0)]);
    let skeleton = thin(thick.data.iter().map(|value| *value >= 0.5).collect(), size);
    for x in 8..30 {
        let column = (0..size[1]).filter(|&y| skeleton[y * size[0] + x]).count();
        assert_eq!(column, 1, "column {x}");
    }
}

#[test]
fn simplify_drops_collinear() {
    let points: Vec<Point> = (0..10).map(|step| [step as f32, 0.0]).collect();
    assert_eq!(simplify(&points, 1.0), vec![[0.0, 0.0], [9.0, 0.0]]);
}

#[test]
fn simplify_keeps_corner() {
    let points = [[0.0, 0.0], [5.0, 0.0], [10.0, 0.0], [10.0, 5.0], [10.0, 10.0]];
    assert_eq!(simplify(&points, 1.0), vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]]);
}

#[test]
fn extract_covers_ink() {
    let strength = canvas(
        120,
        80,
        &[
            ([10.0, 10.0], [110.0, 10.0], 2.0),
            ([20.0, 30.0], [20.0, 75.0], 2.0),
            ([60.0, 40.0], [62.0, 41.0], 1.0),
        ],
    );
    let bands = vec![2; strength.data.len()];
    let strokes = extract(&strength, &bands).unwrap();
    assert!(strokes.iter().any(|stroke| stroke.kind == Kind::Trace));
    let alpha = strength.ink(STROKE.ink_cutoff);
    let mut coverage = vec![false; alpha.len()];
    for stroke in &strokes {
        draw(&mut coverage, [120, 80], stroke);
    }
    assert!(alpha.iter().zip(&coverage).all(|(alpha, hit)| *alpha == 0 || *hit));
}

#[test]
fn extract_rejects_blank() {
    let strength = Plane::filled(32, 32, 0.0);
    assert!(extract(&strength, &vec![2; 32 * 32]).is_err());
}

#[test]
fn bands_split_depth() {
    let farther = Plane { width: 3, height: 1, data: vec![0.0, 0.5, 1.0] };
    assert_eq!(bands(&farther), vec![2, 1, FAR]);
}

#[test]
fn hatch_joins_diagonals() {
    let hatches = hatch(BTreeMap::from([(3, vec![3, 1, 2, 0])]));
    assert_eq!(hatches.len(), 1);
    let reach = STROKE.hatch_reach;
    assert_eq!(
        hatches[0].points,
        vec![
            [centre(0, 3)[0] - reach, centre(0, 3)[1] + reach],
            [centre(3, 0)[0] + reach, centre(3, 0)[1] - reach]
        ]
    );
}

#[test]
fn hatch_splits_long_runs() {
    let sum = STROKE.hatch_run;
    assert_eq!(hatch(BTreeMap::from([(sum, (0..=sum).collect())])).len(), 2);
}

#[test]
fn edge_tiles_covered() {
    let mut strength = Plane::filled(70, 41, 0.0);
    for (x, y) in [(65, 40), (69, 33), (3, 40), (69, 0)] {
        strength.data[y * 70 + x] = 0.1;
    }
    let coverage = covered(&extract(&strength, &vec![2; 70 * 41]).unwrap(), [70, 41]);
    let alpha = strength.ink(STROKE.ink_cutoff);
    assert!(alpha.iter().zip(&coverage).all(|(alpha, hit)| *alpha == 0 || *hit));
}

#[test]
fn sparse_ink_covered() {
    let mut strength = Plane::filled(158, 99, 0.0);
    for (x, y) in [(2, 89), (15, 98)] {
        strength.data[y * 158 + x] = 0.1;
    }
    let strokes = extract(&strength, &vec![2; 158 * 99]).unwrap();
    assert!(strokes.iter().all(|stroke| stroke.kind == Kind::Hatch));
}
