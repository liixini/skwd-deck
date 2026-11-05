use crate::Rgb;

use super::cluster::{Clusters, cluster};
use super::color_space::to_lab;
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

pub fn swatches(rgba: &[u8], width: usize, height: usize, count: usize) -> Vec<Swatch> {
    let samples = sample(rgba, width, height);
    if samples.is_empty() || count == 0 {
        return Vec::new();
    }
    let lab: Vec<(f32, f32, f32)> = samples.iter().map(|(color, _)| to_lab(*color)).collect();
    let weights: Vec<f32> = samples.iter().map(|(_, weight)| *weight).collect();
    let Clusters { centers, owner } = cluster(&lab, &weights, count, LLOYD_PASSES);
    if centers.is_empty() {
        return Vec::new();
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
