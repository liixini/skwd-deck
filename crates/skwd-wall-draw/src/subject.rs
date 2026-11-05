use image::imageops::FilterType;

use crate::models::{Bounds, Detection};
use crate::plane::Plane;
use crate::raster::{self, Point};

struct SceneSpec {
    character_share: f32,
    face_to_head: f32,
    head_reach: f32,
    body_spread: f32,
    eye_line: f32,
    living_confidence: f32,
    living_share: f32,
    near_percent: f32,
    muzzle_width: f32,
    muzzle_rows: [f32; 2],
    keep_share: f32,
    crown_band: f32,
    edge_radius: usize,
    edge_band: [f32; 2],
    outline_radius: usize,
    outline_epsilon: f32,
    outline_keep: f32,
    outline_hole: f32,
}

const SCENE: SceneSpec = SceneSpec {
    character_share: 0.02,
    face_to_head: 1.4,
    head_reach: 1.3,
    body_spread: 1.4,
    eye_line: 0.42,
    living_confidence: 0.4,
    living_share: 0.05,
    near_percent: 55.0,
    muzzle_width: 0.22,
    muzzle_rows: [0.22, 0.62],
    keep_share: 0.1,
    crown_band: 0.12,
    edge_radius: 10,
    edge_band: [0.08, 0.92],
    outline_radius: 8,
    outline_epsilon: 1e-4,
    outline_keep: 0.02,
    outline_hole: 0.0005,
};

pub const INSIDE: f32 = 0.5;

pub struct Found {
    pub faces: Vec<Bounds>,
    pub heads: Vec<Bounds>,
    pub matte: Plane,
    pub detections: Vec<Detection>,
}

pub struct Subject {
    pub head: Option<Bounds>,
    pub mask: Plane,
    pub edge: Plane,
    pub start: Point,
}

impl Subject {
    pub fn new(head: Option<Bounds>, mask: Plane, start: Point) -> Self {
        Self { head, edge: edge(&mask), mask, start }
    }
}

pub fn matte<E>(character: Plane, salient: impl FnOnce() -> Result<Plane, E>) -> Result<Plane, E> {
    if share(&character) >= SCENE.character_share { Ok(character) } else { salient() }
}

pub fn outline(found: &Found, guide: &Plane) -> Plane {
    let cut = found
        .matte
        .resize(guide.width, guide.height, FilterType::Triangle)
        .guided(guide, SCENE.outline_radius, SCENE.outline_epsilon)
        .above(INSIDE);
    fill(&largest(&cut, SCENE.outline_keep), SCENE.outline_hole)
}

fn fill(mask: &Plane, share: f32) -> Plane {
    let limit = share * mask.data.len() as f32;
    let mut filled = mask.clone();
    for pixels in components(&mask.map(|value| 1.0 - value)) {
        if (pixels.len() as f32) < limit {
            for index in pixels {
                filled.data[index] = 1.0;
            }
        }
    }
    filled
}

pub fn find(found: &Found, farther: &Plane) -> Subject {
    let size = [found.matte.width, found.matte.height];
    let mask = &found.matte;
    if let Some(&face) = found.faces.first() {
        let head = found
            .heads
            .iter()
            .find(|head| head.contains(face.middle()))
            .copied()
            .unwrap_or_else(|| grow(face, SCENE.face_to_head, size));
        let reach = grow(head, SCENE.head_reach, size);
        let touching: Vec<Vec<usize>> = components(mask)
            .into_iter()
            .filter(|pixels| {
                pixels.iter().any(|&index| reach.contains(raster::point(index, size[0])))
            })
            .collect();
        let start = [face.middle()[0], face.top + face.height() * SCENE.eye_line];
        let mask = if touching.is_empty() { body(head, size) } else { paint(size, &touching) };
        return Subject::new(Some(head), mask, start);
    }
    let area = (size[0] * size[1]) as f32;
    let living = found
        .detections
        .iter()
        .filter(|hit| {
            hit.confidence >= SCENE.living_confidence
                && hit.bounds.width() * hit.bounds.height() / area >= SCENE.living_share
        })
        .max_by(|left, right| left.confidence.total_cmp(&right.confidence));
    if let Some(living) = living {
        return near(living.bounds, farther);
    }
    let mask = largest(mask, SCENE.keep_share);
    let start = crown(&mask).unwrap_or([size[0] as f32 / 2.0, size[1] as f32 / 2.0]);
    Subject::new(None, mask, start)
}

fn body(head: Bounds, size: [usize; 2]) -> Plane {
    let spread = head.width() * SCENE.body_spread;
    let columns =
        (head.left - spread).max(0.0) as usize..=((head.right + spread) as usize).min(size[0] - 1);
    let mut mask = Plane::filled(size[0], size[1], 0.0);
    for y in head.top as usize..size[1] {
        for x in columns.clone() {
            mask.data[y * size[0] + x] = 1.0;
        }
    }
    mask
}

fn near(bounds: Bounds, farther: &Plane) -> Subject {
    let rows = bounds.top as usize..=bounds.bottom as usize;
    let columns = bounds.left as usize..=bounds.right as usize;
    let inside = Plane {
        width: columns.clone().count(),
        height: rows.clone().count(),
        data: rows
            .clone()
            .flat_map(|y| columns.clone().map(move |x| (x, y)))
            .map(|(x, y)| farther.at(x, y))
            .collect(),
    };
    let cut = inside.percentile(SCENE.near_percent);
    let mut mask = Plane::filled(farther.width, farther.height, 0.0);
    for y in rows {
        for x in columns.clone() {
            if farther.at(x, y) <= cut {
                mask.data[y * farther.width + x] = 1.0;
            }
        }
    }
    let middle = bounds.middle()[0];
    let half = bounds.width() * SCENE.muzzle_width;
    let [top, bottom] = SCENE.muzzle_rows.map(|row| bounds.top + bounds.height() * row);
    let muzzle = Bounds { left: middle - half, top, right: middle + half, bottom };
    Subject::new(Some(muzzle), largest(&mask, 1.0), muzzle.middle())
}

fn crown(mask: &Plane) -> Option<Point> {
    let rows: Vec<usize> =
        (0..mask.height).filter(|&y| (0..mask.width).any(|x| mask.at(x, y) > INSIDE)).collect();
    let (&top, &bottom) = (rows.first()?, rows.last()?);
    let band = top + ((bottom - top) as f32 * SCENE.crown_band) as usize;
    let columns: Vec<f32> = (top..=band)
        .flat_map(|y| (0..mask.width).filter(move |&x| mask.at(x, y) > INSIDE))
        .map(|x| x as f32)
        .collect();
    Some([columns.iter().sum::<f32>() / columns.len() as f32, (top + band) as f32 / 2.0])
}

fn share(mask: &Plane) -> f32 {
    mask.data.iter().filter(|&&value| value > INSIDE).count() as f32 / mask.data.len() as f32
}

fn grow(bounds: Bounds, factor: f32, size: [usize; 2]) -> Bounds {
    let [x, y] = bounds.middle();
    let (half_width, half_height) = (bounds.width() * factor / 2.0, bounds.height() * factor / 2.0);
    let (width, height) = (size[0] as f32 - 1.0, size[1] as f32 - 1.0);
    Bounds {
        left: (x - half_width).clamp(0.0, width),
        top: (y - half_height).clamp(0.0, height),
        right: (x + half_width).clamp(0.0, width),
        bottom: (y + half_height).clamp(0.0, height),
    }
}

fn edge(mask: &Plane) -> Plane {
    let [low, high] = SCENE.edge_band;
    mask.above(INSIDE)
        .mean(SCENE.edge_radius)
        .map(|mean| f32::from(u8::from(mean > low && mean < high)))
}

fn components(mask: &Plane) -> Vec<Vec<usize>> {
    let (width, height) = (mask.width, mask.height);
    let mut seen = vec![false; width * height];
    let mut groups = Vec::new();
    for seed in 0..width * height {
        if seen[seed] || mask.data[seed] <= INSIDE {
            continue;
        }
        seen[seed] = true;
        let mut pixels = vec![seed];
        let mut cursor = 0;
        while let Some(&index) = pixels.get(cursor) {
            cursor += 1;
            let (x, y) = (index % width, index / width);
            let around = [
                (x > 0).then(|| index - 1),
                (x + 1 < width).then(|| index + 1),
                (y > 0).then(|| index - width),
                (y + 1 < height).then(|| index + width),
            ];
            for other in around.into_iter().flatten() {
                if !seen[other] && mask.data[other] > INSIDE {
                    seen[other] = true;
                    pixels.push(other);
                }
            }
        }
        groups.push(pixels);
    }
    groups
}

fn paint(size: [usize; 2], groups: &[Vec<usize>]) -> Plane {
    let mut mask = Plane::filled(size[0], size[1], 0.0);
    for &index in groups.iter().flatten() {
        mask.data[index] = 1.0;
    }
    mask
}

fn largest(mask: &Plane, fraction: f32) -> Plane {
    let groups = components(mask);
    let biggest = groups.iter().map(Vec::len).max().unwrap_or(0) as f32;
    let kept: Vec<Vec<usize>> =
        groups.into_iter().filter(|pixels| pixels.len() as f32 >= biggest * fraction).collect();
    paint([mask.width, mask.height], &kept)
}

#[cfg(test)]
#[path = "subject_tests.rs"]
mod tests;
