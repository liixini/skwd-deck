use std::collections::{BTreeMap, HashSet};

use anyhow::{Result, ensure};

use crate::plane::Plane;
use crate::raster::{self, Point};

pub struct StrokeSpec {
    pub ink_cutoff: f32,
    strength_scale: f32,
    skeleton_cutoff: f32,
    direction_cutoff: f32,
    min_path_points: usize,
    min_length: [f32; LAYERS],
    simplify_far: f32,
    simplify_near: f32,
    depth_score: [f32; LAYERS],
    chunk_points: usize,
    max_traces: usize,
    trace_width: [f32; LAYERS],
    tile: usize,
    hatch_width: f32,
    hatch_run: usize,
    hatch_reach: f32,
    coverage_min: f32,
    band_cuts: [f32; 2],
}

const LAYERS: usize = 3;
const FAR: u8 = 0;
const MIDDLE: u8 = 1;
const NEAR: u8 = 2;
const NEIGHBOURS: [(isize, isize); 8] =
    [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];
const RING: [(isize, isize); 8] =
    [(0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1)];
const MASK_COVERAGE: f32 = 0.5;
const THINNING_NEIGHBOURS: std::ops::RangeInclusive<usize> = 2..=6;

pub const STROKE: StrokeSpec = StrokeSpec {
    ink_cutoff: 0.02,
    strength_scale: 4.0,
    skeleton_cutoff: 0.70,
    direction_cutoff: 0.35,
    min_path_points: 4,
    min_length: [30.0, 9.0, 6.0],
    simplify_far: 1.6,
    simplify_near: 1.1,
    depth_score: [1.0, 1.15, 1.3],
    chunk_points: 81,
    max_traces: 7000,
    trace_width: [8.0, 7.0, 7.0],
    tile: 16,
    hatch_width: 24.0,
    hatch_run: 6,
    hatch_reach: 6.4,
    coverage_min: 0.99,
    band_cuts: [32.0, 68.0],
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Trace,
    Hatch,
}

#[derive(Clone, Debug)]
pub struct Stroke {
    pub points: Vec<Point>,
    pub width: f32,
    pub kind: Kind,
}

pub fn bands(farther: &Plane) -> Vec<u8> {
    let [near, far] = STROKE.band_cuts.map(|percent| farther.percentile(percent));
    farther
        .data
        .iter()
        .map(|&value| {
            if value >= far {
                FAR
            } else if value >= near {
                MIDDLE
            } else {
                NEAR
            }
        })
        .collect()
}

pub fn draw(mask: &mut [bool], size: [usize; 2], stroke: &Stroke) {
    for pair in stroke.points.windows(2) {
        raster::stamp(pair[0], pair[1], stroke.width, size, |index, coverage, _| {
            if coverage >= MASK_COVERAGE {
                mask[index] = true;
            }
        });
    }
}

pub fn extract(strength: &Plane, bands: &[u8]) -> Result<Vec<Stroke>> {
    let size = [strength.width, strength.height];
    let ink: Vec<bool> = strength
        .data
        .iter()
        .map(|value| value * STROKE.strength_scale >= STROKE.skeleton_cutoff)
        .collect();
    let mut ranked = Vec::new();
    for path in trace_paths(&thin(ink, size), size) {
        if path.len() < STROKE.min_path_points {
            continue;
        }
        let points: Vec<Point> = path.iter().map(|&index| raster::point(index, size[0])).collect();
        let middle = raster::centre(&points);
        let layer =
            usize::from(bands[middle[1].round() as usize * size[0] + middle[0].round() as usize]);
        let length = raster::length(&points);
        if length < STROKE.min_length[layer] {
            continue;
        }
        let tolerance =
            if layer == usize::from(FAR) { STROKE.simplify_far } else { STROKE.simplify_near };
        let simpler = simplify(&points, tolerance);
        if simpler.len() < 2 {
            continue;
        }
        let score = length * strength.sample(middle) * STROKE.depth_score[layer];
        for start in (0..simpler.len() - 1).step_by(STROKE.chunk_points - 1) {
            let chunk = &simpler[start..(start + STROKE.chunk_points).min(simpler.len())];
            if chunk.len() > 1 {
                let stroke = Stroke {
                    points: chunk.to_vec(),
                    width: STROKE.trace_width[layer],
                    kind: Kind::Trace,
                };
                ranked.push((score, stroke));
            }
        }
    }
    ranked.sort_by(|left, right| right.0.total_cmp(&left.0));
    let mut strokes: Vec<Stroke> =
        ranked.into_iter().take(STROKE.max_traces).map(|(_, stroke)| stroke).collect();
    let alpha = strength.ink(STROKE.ink_cutoff);
    ensure!(alpha.iter().any(|&value| value > 0), "lineart produced no visible ink");
    let mut coverage = covered(&strokes, size);
    let mut diagonals: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for top in (0..size[1]).step_by(STROKE.tile) {
        for left in (0..size[0]).step_by(STROKE.tile) {
            let right = (left + STROKE.tile).min(size[0]);
            let bottom = (top + STROKE.tile).min(size[1]);
            let uncovered = (top..bottom).any(|y| {
                (left..right).any(|x| alpha[y * size[0] + x] > 0 && !coverage[y * size[0] + x])
            });
            if !uncovered {
                continue;
            }
            let (column, row) = (left / STROKE.tile, top / STROKE.tile);
            draw(&mut coverage, size, &band(centre(column, row), centre(column, row)));
            diagonals.entry(column + row).or_default().push(column);
        }
    }
    strokes.extend(hatch(diagonals));
    let coverage = covered(&strokes, size);
    let total: f32 = alpha.iter().map(|&value| f32::from(value)).sum();
    let reached: f32 = alpha
        .iter()
        .zip(&coverage)
        .filter(|(_, hit)| **hit)
        .map(|(&value, _)| f32::from(value))
        .sum();
    ensure!(
        reached / total >= STROKE.coverage_min,
        "drawing ink coverage is {:.4}",
        reached / total
    );
    Ok(strokes)
}

fn covered(strokes: &[Stroke], size: [usize; 2]) -> Vec<bool> {
    let mut coverage = vec![false; size[0] * size[1]];
    for stroke in strokes {
        draw(&mut coverage, size, stroke);
    }
    coverage
}

fn band(first: Point, last: Point) -> Stroke {
    let reach = STROKE.hatch_reach;
    let points = vec![[first[0] - reach, first[1] + reach], [last[0] + reach, last[1] - reach]];
    Stroke { points, width: STROKE.hatch_width, kind: Kind::Hatch }
}

fn centre(column: usize, row: usize) -> Point {
    let tile = STROKE.tile as f32;
    [(column as f32 + 0.5) * tile, (row as f32 + 0.5) * tile]
}

fn hatch(diagonals: BTreeMap<usize, Vec<usize>>) -> Vec<Stroke> {
    let mut hatches = Vec::new();
    for (sum, mut columns) in diagonals {
        columns.sort_unstable();
        let mut runs: Vec<Vec<usize>> = Vec::new();
        for column in columns {
            match runs.last_mut() {
                Some(run)
                    if run.last().is_some_and(|&last| last + 1 == column)
                        && run.len() < STROKE.hatch_run =>
                {
                    run.push(column);
                }
                _ => runs.push(vec![column]),
            }
        }
        for run in runs {
            let [first, last] =
                [run[0], run[run.len() - 1]].map(|column| centre(column, sum - column));
            hatches.push(band(first, last));
        }
    }
    hatches
}

fn offset(index: usize, (dx, dy): (isize, isize), size: [usize; 2]) -> Option<usize> {
    let x = (index % size[0]).checked_add_signed(dx)?;
    let y = (index / size[0]).checked_add_signed(dy)?;
    (x < size[0] && y < size[1]).then_some(y * size[0] + x)
}

fn neighbours(index: usize, size: [usize; 2]) -> impl Iterator<Item = usize> {
    NEIGHBOURS.into_iter().filter_map(move |step| offset(index, step, size))
}

fn first_max<T>(candidates: impl Iterator<Item = (T, f32)>) -> Option<(T, f32)> {
    candidates.fold(None, |best, candidate| match best {
        Some(kept) if kept.1 >= candidate.1 => Some(kept),
        _ => Some(candidate),
    })
}

fn thin(mut mask: Vec<bool>, size: [usize; 2]) -> Vec<bool> {
    let ring = |mask: &[bool], index: usize| {
        RING.map(|step| offset(index, step, size).is_some_and(|other| mask[other]))
    };
    loop {
        let mut changed = false;
        for pass in 0..2 {
            let mut removals = Vec::new();
            for y in 0..size[1] {
                for x in 0..size[0] {
                    if !mask[y * size[0] + x] {
                        continue;
                    }
                    let around = ring(&mask, y * size[0] + x);
                    let filled = around.iter().filter(|&&set| set).count();
                    let rises = (0..around.len())
                        .filter(|&step| !around[step] && around[(step + 1) % around.len()])
                        .count();
                    let [north, _, east, _, south, _, west, _] = around;
                    let clear = if pass == 0 {
                        !(east && south && (north || west))
                    } else {
                        !(north && west && (east || south))
                    };
                    if THINNING_NEIGHBOURS.contains(&filled) && rises == 1 && clear {
                        removals.push(y * size[0] + x);
                    }
                }
            }
            changed |= !removals.is_empty();
            for index in removals {
                mask[index] = false;
            }
        }
        if !changed {
            return mask;
        }
    }
}

fn trace_paths(skeleton: &[bool], size: [usize; 2]) -> Vec<Vec<usize>> {
    let linked =
        |index: usize| neighbours(index, size).filter(|&other| skeleton[other]).collect::<Vec<_>>();
    let edge = |from: usize, to: usize| (from.min(to), from.max(to));
    let mut visited = HashSet::new();
    let follow = |start: usize, next: usize, visited: &mut HashSet<(usize, usize)>| {
        let mut path = vec![start];
        let (mut previous, mut current) = (start, next);
        while visited.insert(edge(previous, current)) {
            path.push(current);
            let (from, here) = (raster::point(previous, size[0]), raster::point(current, size[0]));
            let heading = [here[0] - from[0], here[1] - from[1]];
            let best = first_max(
                linked(current)
                    .into_iter()
                    .filter(|&other| other != previous && !visited.contains(&edge(current, other)))
                    .map(|other| {
                        let to = raster::point(other, size[0]);
                        let step = [to[0] - here[0], to[1] - here[1]];
                        let alignment =
                            (heading[0] * step[0] + heading[1] * step[1]) / step[0].hypot(step[1]);
                        (other, alignment)
                    }),
            );
            let Some((other, alignment)) = best else { break };
            if alignment / heading[0].hypot(heading[1]) < STROKE.direction_cutoff {
                break;
            }
            (previous, current) = (current, other);
        }
        path
    };
    let order: Vec<usize> = (0..size[0])
        .flat_map(|x| (0..size[1]).map(move |y| y * size[0] + x))
        .filter(|&index| skeleton[index])
        .collect();
    let mut paths = Vec::new();
    for &start in &order {
        let links = linked(start);
        if links.len() == 2 {
            continue;
        }
        for next in links {
            if !visited.contains(&edge(start, next)) {
                paths.push(follow(start, next, &mut visited));
            }
        }
    }
    for &start in &order {
        for next in linked(start) {
            if !visited.contains(&edge(start, next)) {
                paths.push(follow(start, next, &mut visited));
            }
        }
    }
    paths
}

fn simplify(points: &[Point], tolerance: f32) -> Vec<Point> {
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut spans = vec![(0, points.len() - 1)];
    while let Some((first, last)) = spans.pop() {
        let (start, end) = (points[first], points[last]);
        let chord = raster::distance(start, end);
        let farthest = first_max((first + 1..last).map(|index| {
            let point = points[index];
            let offset = if chord <= f32::EPSILON {
                raster::distance(point, start)
            } else {
                ((end[0] - start[0]) * (start[1] - point[1])
                    - (start[0] - point[0]) * (end[1] - start[1]))
                    .abs()
                    / chord
            };
            (index, offset)
        }));
        if let Some((index, offset)) = farthest
            && offset > tolerance
        {
            keep[index] = true;
            spans.push((first, index));
            spans.push((index, last));
        }
    }
    points.iter().zip(keep).filter(|(_, kept)| *kept).map(|(point, _)| *point).collect()
}

#[cfg(test)]
#[path = "strokes_tests.rs"]
mod tests;
