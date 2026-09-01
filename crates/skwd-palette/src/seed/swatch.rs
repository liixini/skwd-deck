use crate::Rgb;

use super::color_space::{distance_squared, to_lab};
use super::sampling::sample;

pub const ACHROMATIC_CHROMA: f32 = 8.0;
pub const MIN_ACCENT_SHARE: f32 = 0.03;
const LLOYD_PASSES: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Swatch {
    pub color: Rgb,
    pub share: f32,
    pub chroma: f32,
    pub hue: f32,
    pub sat: f32,
    pub light: f32,
}

fn initial_centers(lab: &[(f32, f32, f32)], count: usize) -> Vec<(f32, f32, f32)> {
    let mut centers = Vec::with_capacity(count);
    let mean = lab.iter().fold((0.0f64, 0.0f64, 0.0f64), |acc, point| {
        (acc.0 + f64::from(point.0), acc.1 + f64::from(point.1), acc.2 + f64::from(point.2))
    });
    let sample_count = lab.len() as f64;
    let mean = (
        (mean.0 / sample_count) as f32,
        (mean.1 / sample_count) as f32,
        (mean.2 / sample_count) as f32,
    );
    let first = lab
        .iter()
        .copied()
        .min_by(|a, b| distance_squared(*a, mean).total_cmp(&distance_squared(*b, mean)))
        .unwrap_or(mean);
    centers.push(first);

    let mut nearest: Vec<f32> = lab.iter().map(|point| distance_squared(*point, first)).collect();
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
        let next = lab[index];
        centers.push(next);
        for (slot, point) in nearest.iter_mut().zip(lab) {
            *slot = slot.min(distance_squared(*point, next));
        }
    }
    centers
}

pub fn swatches(rgba: &[u8], width: usize, height: usize, count: usize) -> Vec<Swatch> {
    let samples = sample(rgba, width, height);
    if samples.is_empty() || count == 0 {
        return Vec::new();
    }
    let lab: Vec<(f32, f32, f32)> = samples.iter().map(|(color, _)| to_lab(*color)).collect();
    let mut centers = initial_centers(&lab, count);
    if centers.is_empty() {
        return Vec::new();
    }
    let mut owner = vec![usize::MAX; lab.len()];

    for _ in 0..LLOYD_PASSES {
        let mut moved = false;
        for (index, point) in lab.iter().enumerate() {
            let mut best = 0usize;
            let mut best_distance = f32::MAX;
            for (center_id, center) in centers.iter().enumerate() {
                let distance = distance_squared(*point, *center);
                if distance < best_distance {
                    best_distance = distance;
                    best = center_id;
                }
            }
            if owner[index] != best {
                owner[index] = best;
                moved = true;
            }
        }
        if !moved {
            break;
        }
        let mut sums = vec![(0.0f64, 0.0f64, 0.0f64, 0.0f64); centers.len()];
        for (index, point) in lab.iter().enumerate() {
            let weight = f64::from(samples[index].1);
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

    let mut accumulators = vec![Accumulator::default(); centers.len()];
    for (index, (color, weight)) in samples.iter().enumerate() {
        let weight = f64::from(*weight);
        let (hue, saturation, light) = crate::to_hsl(*color);
        let point = lab[index];
        let slot = &mut accumulators[owner[index]];
        slot.red += f64::from(color.0) * weight;
        slot.green += f64::from(color.1) * weight;
        slot.blue += f64::from(color.2) * weight;
        let radians = f64::from(hue).to_radians();
        slot.hue_sin += radians.sin() * weight;
        slot.hue_cos += radians.cos() * weight;
        slot.saturation += f64::from(saturation) * weight;
        slot.light += f64::from(light) * weight;
        slot.chroma += f64::from(point.1.hypot(point.2)) * weight;
        slot.mass += weight;
    }
    let total: f64 = accumulators.iter().map(|slot| slot.mass).sum();
    if total <= 0.0 {
        return Vec::new();
    }
    let mut output: Vec<Swatch> = accumulators
        .iter()
        .filter(|slot| slot.mass > 0.0)
        .map(|slot| {
            let mean = |sum: f64| (sum / slot.mass) as f32;
            Swatch {
                color: Rgb(
                    (slot.red / slot.mass).round() as u8,
                    (slot.green / slot.mass).round() as u8,
                    (slot.blue / slot.mass).round() as u8,
                ),
                share: (slot.mass / total) as f32,
                chroma: mean(slot.chroma),
                hue: slot.hue_sin.atan2(slot.hue_cos).to_degrees().rem_euclid(360.0) as f32,
                sat: mean(slot.saturation),
                light: mean(slot.light),
            }
        })
        .collect();
    output.sort_by(|a, b| b.share.total_cmp(&a.share));
    output
}

#[derive(Default, Clone, Copy)]
struct Accumulator {
    red: f64,
    green: f64,
    blue: f64,
    hue_sin: f64,
    hue_cos: f64,
    saturation: f64,
    light: f64,
    chroma: f64,
    mass: f64,
}

pub fn pick(swatches: &[Swatch]) -> Option<Swatch> {
    swatches
        .iter()
        .filter(|swatch| swatch.chroma >= ACHROMATIC_CHROMA && swatch.share >= MIN_ACCENT_SHARE)
        .copied()
        .max_by(|a, b| score(a).total_cmp(&score(b)))
        .or_else(|| swatches.iter().copied().max_by(|a, b| a.share.total_cmp(&b.share)))
}

fn score(swatch: &Swatch) -> f32 {
    let (light, _, _) = to_lab(swatch.color);
    let centrality = 1.0 - ((light - 55.0) / 55.0).abs().min(1.0);
    swatch.chroma * swatch.share.sqrt() * (0.35 + 0.65 * centrality)
}

pub fn seed(rgba: &[u8], width: usize, height: usize, count: usize) -> Option<Rgb> {
    pick(&swatches(rgba, width, height, count)).map(|swatch| swatch.color)
}

#[cfg(test)]
#[path = "swatch_tests.rs"]
mod tests;
