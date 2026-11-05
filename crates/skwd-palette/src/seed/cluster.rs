pub type Point = (f32, f32, f32);

pub struct Clusters {
    pub centers: Vec<Point>,
    pub owner: Vec<usize>,
}

pub fn distance_squared(a: Point, b: Point) -> f32 {
    (a.0 - b.0).powi(2) + (a.1 - b.1).powi(2) + (a.2 - b.2).powi(2)
}

fn initial_centers(points: &[Point], count: usize) -> Vec<Point> {
    let mut centers = Vec::with_capacity(count);
    let sum = points.iter().fold((0.0f64, 0.0f64, 0.0f64), |acc, point| {
        (acc.0 + f64::from(point.0), acc.1 + f64::from(point.1), acc.2 + f64::from(point.2))
    });
    let total = points.len() as f64;
    let mean = ((sum.0 / total) as f32, (sum.1 / total) as f32, (sum.2 / total) as f32);
    let first = points
        .iter()
        .copied()
        .min_by(|a, b| distance_squared(*a, mean).total_cmp(&distance_squared(*b, mean)))
        .unwrap_or(mean);
    centers.push(first);

    let mut nearest: Vec<f32> =
        points.iter().map(|point| distance_squared(*point, first)).collect();
    while centers.len() < count {
        let Some((index, farthest)) = nearest
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(index, distance)| (index, *distance))
        else {
            break;
        };
        if farthest <= f32::EPSILON {
            break;
        }
        let next = points[index];
        centers.push(next);
        for (slot, point) in nearest.iter_mut().zip(points) {
            *slot = slot.min(distance_squared(*point, next));
        }
    }
    centers
}

fn nearest_center(point: Point, centers: &[Point]) -> usize {
    let mut best = 0usize;
    let mut best_distance = f32::MAX;
    for (center_id, center) in centers.iter().enumerate() {
        let distance = distance_squared(point, *center);
        if distance < best_distance {
            best_distance = distance;
            best = center_id;
        }
    }
    best
}

pub fn cluster(points: &[Point], weights: &[f32], count: usize, passes: usize) -> Clusters {
    let mut centers = initial_centers(points, count);
    let mut owner = vec![usize::MAX; points.len()];
    if centers.is_empty() {
        return Clusters { centers, owner };
    }
    for _ in 0..passes {
        let mut moved = false;
        for (index, point) in points.iter().enumerate() {
            let best = nearest_center(*point, &centers);
            if owner[index] != best {
                owner[index] = best;
                moved = true;
            }
        }
        if !moved {
            break;
        }
        let mut sums = vec![(0.0f64, 0.0f64, 0.0f64, 0.0f64); centers.len()];
        for (index, point) in points.iter().enumerate() {
            let weight = f64::from(weights[index]);
            let slot = &mut sums[owner[index]];
            slot.0 += f64::from(point.0) * weight;
            slot.1 += f64::from(point.1) * weight;
            slot.2 += f64::from(point.2) * weight;
            slot.3 += weight;
        }
        for (center, sum) in centers.iter_mut().zip(&sums) {
            if sum.3 > 0.0 {
                *center = ((sum.0 / sum.3) as f32, (sum.1 / sum.3) as f32, (sum.2 / sum.3) as f32);
            }
        }
    }
    Clusters { centers, owner }
}
