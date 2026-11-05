use crate::Rgb;
use crate::seed::{Clusters, Point, cluster};

use super::oklch::{LAB_AXES, Lab, Oklch, from_oklch, hue_distance, lab_to_lch, to_oklab};

pub const ANALYSIS_EDGE: u32 = 192;
const CLUSTER_COUNT: usize = 12;
const CLUSTER_PASSES: usize = 14;
const CHROMATIC_CHROMA: f32 = 0.035;
const CHROMATIC_SHARE: f32 = 0.003;
const GREY_SHARE: f32 = 0.02;
const BLUR_SIGMA: f32 = 1.5;
const BLUR_REACH: f32 = 3.0;
const CENTRE_SPREAD: f32 = 0.45;
const CENTRE_FLOOR: f32 = 0.5;
const RGBA: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cluster {
    pub id: usize,
    pub lab: Lab,
    pub lch: Oklch,
    pub share: f32,
    pub x: f32,
    pub y: f32,
}

impl Cluster {
    pub fn color(&self) -> Rgb {
        from_oklch(self.lch)
    }

    pub fn weight(&self) -> f32 {
        self.lch.chroma * self.share.sqrt()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Area {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Area {
    pub fn edges(self) -> [f32; 4] {
        [self.left, self.top, self.right, self.bottom]
    }

    pub fn from_edges(edges: &[f32]) -> Option<Self> {
        let [left, top, right, bottom] = *edges else {
            return None;
        };
        (left < right && top < bottom).then_some(Self { left, top, right, bottom })
    }
}

pub struct Analysis {
    pub width: usize,
    pub height: usize,
    pub lab: Vec<Lab>,
    pub owner: Vec<usize>,
    pub clusters: Vec<Cluster>,
    pub mean: Lab,
    pub grey: bool,
}

fn as_point(lab: Lab) -> Point {
    (lab[0], lab[1], lab[2])
}

pub fn crop(rgba: &[u8], width: usize, height: usize, area: Area) -> (Vec<u8>, usize, usize) {
    let span = |from: f32, to: f32, size: usize| {
        let start =
            ((from.clamp(0.0, 1.0) * size as f32).floor() as usize).min(size.saturating_sub(1));
        let end = ((to.clamp(0.0, 1.0) * size as f32).ceil() as usize).clamp(start + 1, size);
        (start, end)
    };
    let (left, right) = span(area.left, area.right, width);
    let (top, bottom) = span(area.top, area.bottom, height);
    let mut out = Vec::with_capacity((right - left) * (bottom - top) * RGBA);
    for row in top..bottom {
        let start = (row * width + left) * RGBA;
        out.extend_from_slice(&rgba[start..start + (right - left) * RGBA]);
    }
    (out, right - left, bottom - top)
}

impl Analysis {
    pub fn new(rgba: &[u8], width: usize, height: usize) -> Option<Self> {
        let lab: Vec<Lab> = rgba
            .chunks_exact(RGBA)
            .take(width * height)
            .map(|pixel| to_oklab(Rgb(pixel[0], pixel[1], pixel[2])))
            .collect();
        if lab.is_empty() {
            return None;
        }
        let points: Vec<Point> = lab.iter().copied().map(as_point).collect();
        let weights = vec![1.0; points.len()];
        let Clusters { centers, owner } = cluster(&points, &weights, CLUSTER_COUNT, CLUSTER_PASSES);
        let total = lab.len() as f32;
        let mut counts = vec![(0usize, 0.0f32, 0.0f32); centers.len()];
        for (index, &id) in owner.iter().enumerate() {
            let slot = &mut counts[id];
            slot.0 += 1;
            slot.1 += (index % width) as f32;
            slot.2 += (index / width) as f32;
        }
        let clusters: Vec<Cluster> = centers
            .iter()
            .zip(&counts)
            .enumerate()
            .filter(|(_, (_, count))| count.0 > 0)
            .map(|(id, (center, count))| {
                let lab = [center.0, center.1, center.2];
                let members = count.0 as f32;
                Cluster {
                    id,
                    lab,
                    lch: lab_to_lch(lab),
                    share: members / total,
                    x: count.1 / members / width as f32,
                    y: count.2 / members / height as f32,
                }
            })
            .collect();
        let sum = lab.iter().fold([0.0f32; LAB_AXES], |acc, pixel| {
            [acc[0] + pixel[0], acc[1] + pixel[1], acc[2] + pixel[2]]
        });
        let mean = sum.map(|channel| channel / total);
        let mut analysis = Self { width, height, lab, owner, clusters, mean, grey: false };
        analysis.grey = analysis.chromatic().map(|found| found.share).sum::<f32>() < GREY_SHARE;
        Some(analysis)
    }

    pub fn chromatic(&self) -> impl Iterator<Item = &Cluster> {
        self.clusters
            .iter()
            .filter(|found| found.lch.chroma > CHROMATIC_CHROMA && found.share >= CHROMATIC_SHARE)
    }

    pub fn seed(&self) -> Cluster {
        self.chromatic().copied().max_by(|a, b| a.weight().total_cmp(&b.weight())).unwrap_or(
            Cluster {
                id: usize::MAX,
                lab: self.mean,
                lch: lab_to_lch(self.mean),
                share: 1.0,
                x: 0.5,
                y: 0.5,
            },
        )
    }

    pub fn others(&self, base_hue: f32, count: usize, min_gap: f32) -> Vec<Cluster> {
        let mut ranked: Vec<Cluster> = self.chromatic().copied().collect();
        ranked.sort_by(|a, b| b.weight().total_cmp(&a.weight()));
        let mut picked: Vec<Cluster> = Vec::with_capacity(count);
        for candidate in ranked {
            if picked.len() == count {
                break;
            }
            let clear = std::iter::once(base_hue)
                .chain(picked.iter().map(|found| found.lch.hue))
                .all(|hue| hue_distance(candidate.lch.hue, hue) >= min_gap);
            if clear {
                picked.push(candidate);
            }
        }
        picked
    }

    pub fn largest(&self) -> Cluster {
        self.clusters
            .iter()
            .copied()
            .max_by(|a, b| a.share.total_cmp(&b.share))
            .unwrap_or_else(|| self.seed())
    }

    pub fn region(&self, area: Area) -> Lab {
        let cols =
            ((area.left * self.width as f32) as usize, (area.right * self.width as f32) as usize);
        let rows =
            ((area.top * self.height as f32) as usize, (area.bottom * self.height as f32) as usize);
        let (mut sum, mut count) = ([0.0f32; LAB_AXES], 0.0f32);
        for row in rows.0..rows.1.max(rows.0 + 1).min(self.height) {
            for col in cols.0..cols.1.min(self.width) {
                let pixel = self.lab[row * self.width + col];
                for (slot, value) in sum.iter_mut().zip(pixel) {
                    *slot += value;
                }
                count += 1.0;
            }
        }
        if count == 0.0 { self.mean } else { sum.map(|channel| channel / count) }
    }

    pub fn saliency(&self) -> Vec<f32> {
        let blurred = blur(&self.lab, self.width, self.height);
        blurred
            .iter()
            .enumerate()
            .map(|(index, pixel)| {
                let distance = pixel
                    .iter()
                    .zip(self.mean)
                    .map(|(value, mean)| (value - mean).powi(2))
                    .sum::<f32>()
                    .sqrt();
                let dx = ((index % self.width) as f32 / self.width as f32 - 0.5) / CENTRE_SPREAD;
                let dy = ((index / self.width) as f32 / self.height as f32 - 0.5) / CENTRE_SPREAD;
                let centre = (-(dx * dx + dy * dy)).exp();
                distance * (CENTRE_FLOOR + (1.0 - CENTRE_FLOOR) * centre)
            })
            .collect()
    }
}

fn kernel() -> Vec<f32> {
    let reach = (BLUR_SIGMA * BLUR_REACH).ceil() as i32;
    let weights: Vec<f32> = (-reach..=reach)
        .map(|offset| (-(offset * offset) as f32 / (2.0 * BLUR_SIGMA * BLUR_SIGMA)).exp())
        .collect();
    let total: f32 = weights.iter().sum();
    weights.into_iter().map(|weight| weight / total).collect()
}

fn blur(lab: &[Lab], width: usize, height: usize) -> Vec<Lab> {
    let weights = kernel();
    let reach = (weights.len() / 2) as isize;
    let pass = |source: &[Lab], horizontal: bool| -> Vec<Lab> {
        (0..source.len())
            .map(|index| {
                let (col, row) = ((index % width) as isize, (index / width) as isize);
                let mut acc = [0.0f32; LAB_AXES];
                for (tap, weight) in weights.iter().enumerate() {
                    let offset = tap as isize - reach;
                    let (sample_col, sample_row) = if horizontal {
                        ((col + offset).clamp(0, width as isize - 1), row)
                    } else {
                        (col, (row + offset).clamp(0, height as isize - 1))
                    };
                    let sample = source[sample_row as usize * width + sample_col as usize];
                    for (slot, value) in acc.iter_mut().zip(sample) {
                        *slot += value * weight;
                    }
                }
                acc
            })
            .collect()
    };
    pass(&pass(lab, true), false)
}

#[cfg(test)]
#[path = "analysis_tests.rs"]
mod tests;
