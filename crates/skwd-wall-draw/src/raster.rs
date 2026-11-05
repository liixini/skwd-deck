pub type Point = [f32; 2];

pub fn stamp(
    start: Point,
    end: Point,
    width: f32,
    size: [usize; 2],
    mut visit: impl FnMut(usize, f32, f32),
) {
    let half = width.round().max(1.0) / 2.0;
    let reach = half + 1.0;
    let bound = |low: f32, high: f32, limit: usize| {
        let first = (low - reach).floor().max(0.0) as usize;
        let last = ((high + reach).ceil().max(0.0) as usize).min(limit - 1);
        first..=last
    };
    let columns = bound(start[0].min(end[0]), start[0].max(end[0]), size[0]);
    let rows = bound(start[1].min(end[1]), start[1].max(end[1]), size[1]);
    let delta = [end[0] - start[0], end[1] - start[1]];
    let span = (delta[0] * delta[0] + delta[1] * delta[1]).max(f32::EPSILON);
    for y in rows {
        for x in columns.clone() {
            let offset = [x as f32 - start[0], y as f32 - start[1]];
            let fraction = ((offset[0] * delta[0] + offset[1] * delta[1]) / span).clamp(0.0, 1.0);
            let dx = offset[0] - fraction * delta[0];
            let dy = offset[1] - fraction * delta[1];
            let coverage = (half + 0.5 - dx.hypot(dy)).clamp(0.0, 1.0);
            if coverage > 0.0 {
                visit(y * size[0] + x, coverage, fraction);
            }
        }
    }
}

pub fn point(index: usize, width: usize) -> Point {
    [(index % width) as f32, (index / width) as f32]
}

pub fn length(points: &[Point]) -> f32 {
    points.windows(2).map(|pair| (pair[1][0] - pair[0][0]).hypot(pair[1][1] - pair[0][1])).sum()
}

pub fn centre(points: &[Point]) -> Point {
    let count = points.len() as f32;
    let sum = points.iter().fold([0.0, 0.0], |sum, point| [sum[0] + point[0], sum[1] + point[1]]);
    [sum[0] / count, sum[1] / count]
}

pub fn distance(left: Point, right: Point) -> f32 {
    (left[0] - right[0]).hypot(left[1] - right[1])
}

#[cfg(test)]
#[path = "raster_tests.rs"]
mod tests;
