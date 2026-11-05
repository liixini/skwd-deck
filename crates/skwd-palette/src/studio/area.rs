use crate::Rgb;
use crate::seed::{Clusters, Point, cluster};

use super::analysis::{Area, crop};
use super::oklch::{FULL_TURN, Oklch, hue_distance, lab_to_lch, oklch, to_oklab};
use super::palette::{ANSI_SLOTS, StudioPalette};
use super::polish::{Pull, WALLPAPER_TERMINAL, fit, polish, terminal};

pub const AREA_EDGE: u32 = 240;
const SAMPLE_POINTS: f32 = 3000.0;
const CLUSTER_COUNT: usize = 6;
const CLUSTER_PASSES: usize = 8;
const CHROMATIC_CHROMA: f32 = 0.035;
const CHROMATIC_SHARE: f32 = 0.01;
const GREY_SHARE: f32 = 0.02;
const OTHER_GAP: f32 = 40.0;
const GREY_CHROMA: f32 = 0.02;
const ACCENT_CHROMA: f32 = 0.19;
const SECOND_CHROMA: f32 = 0.12;
const THIRD_CHROMA: f32 = 0.11;
const SECOND_FALLBACK: (f32, f32) = (30.0, 0.07);
const THIRD_FALLBACK: (f32, f32) = (60.0, 0.08);
const TARGET: f32 = 4.5;
const WHEEL_BINS: usize = 36;
const DEGREES_PER_BIN: f32 = 10.0;
const RGBA: usize = 4;

struct Spec {
    base: (f32, f32),
    panel: (f32, f32),
    raised: (f32, f32),
    text: (f32, f32),
    quiet: (f32, f32),
    on_accent: (f32, f32),
    second: f32,
    third: f32,
}

const DARK: Spec = Spec {
    base: (0.17, 0.012),
    panel: (0.21, 0.014),
    raised: (0.26, 0.016),
    text: (0.93, 0.012),
    quiet: (0.66, 0.02),
    on_accent: (0.22, 0.05),
    second: 0.74,
    third: 0.82,
};

const LIGHT: Spec = Spec {
    base: (0.975, 0.008),
    panel: (0.945, 0.012),
    raised: (0.905, 0.016),
    text: (0.22, 0.015),
    quiet: (0.5, 0.02),
    on_accent: (0.98, 0.01),
    second: 0.5,
    third: 0.42,
};

const DARK_ACCENT_FLOOR: f32 = 0.7;
const LIGHT_ACCENT_CEILING: f32 = 0.52;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Patch {
    pub lch: Oklch,
    pub share: f32,
}

impl Patch {
    fn rank(&self) -> f32 {
        self.lch.chroma * self.share.sqrt()
    }
}

pub fn patches(rgba: &[u8], width: usize, height: usize, area: Area) -> Vec<Patch> {
    let (pixels, width, height) = crop(rgba, width, height, area);
    let stride = ((width * height) as f32 / SAMPLE_POINTS).sqrt().ceil().max(1.0) as usize;
    let mut points: Vec<Point> = Vec::new();
    for row in (0..height).step_by(stride) {
        for col in (0..width).step_by(stride) {
            let start = (row * width + col) * RGBA;
            let lab = to_oklab(Rgb(pixels[start], pixels[start + 1], pixels[start + 2]));
            points.push((lab[0], lab[1], lab[2]));
        }
    }
    if points.is_empty() {
        return Vec::new();
    }
    let weights = vec![1.0; points.len()];
    let Clusters { centers, owner } = cluster(&points, &weights, CLUSTER_COUNT, CLUSTER_PASSES);
    let total = points.len() as f32;
    let mut found: Vec<Patch> = centers
        .iter()
        .enumerate()
        .map(|(id, center)| Patch {
            lch: lab_to_lch([center.0, center.1, center.2]),
            share: owner.iter().filter(|&&owned| owned == id).count() as f32 / total,
        })
        .filter(|patch| patch.share > 0.0)
        .collect();
    found.sort_by(|a, b| b.share.total_cmp(&a.share));
    found
}

fn chromatic(found: &[Patch]) -> Vec<Patch> {
    found
        .iter()
        .copied()
        .filter(|patch| patch.lch.chroma > CHROMATIC_CHROMA && patch.share >= CHROMATIC_SHARE)
        .collect()
}

pub fn seed(found: &[Patch]) -> Option<Patch> {
    let colourful = chromatic(found);
    colourful
        .iter()
        .copied()
        .fold(None, |best: Option<Patch>, patch| match best {
            Some(kept) if kept.rank() >= patch.rank() => Some(kept),
            _ => Some(patch),
        })
        .or_else(|| found.first().copied())
}

pub fn wheel(found: &[Patch]) -> [f32; WHEEL_BINS] {
    let mut bins = [0.0f32; WHEEL_BINS];
    for patch in found.iter().filter(|patch| patch.lch.chroma > CHROMATIC_CHROMA) {
        bins[(patch.lch.hue / DEGREES_PER_BIN) as usize % WHEEL_BINS] +=
            patch.share * patch.lch.chroma;
    }
    let peak = bins.iter().copied().fold(0.0, f32::max);
    if peak > 0.0 {
        for bin in &mut bins {
            *bin /= peak;
        }
    }
    bins
}

pub fn palette(found: &[Patch], dark: bool) -> Option<StudioPalette> {
    let spec = if dark { &DARK } else { &LIGHT };
    let colourful = chromatic(found);
    let grey = colourful.iter().map(|patch| patch.share).sum::<f32>() < GREY_SHARE;
    let seed = seed(found)?;
    let hue = seed.lch.hue;
    let mut ranked = colourful.clone();
    ranked.sort_by(|a, b| b.rank().total_cmp(&a.rank()));
    let mut others: Vec<Patch> = Vec::new();
    for candidate in ranked {
        let clear = std::iter::once(hue)
            .chain(others.iter().map(|other| other.lch.hue))
            .all(|taken| hue_distance(candidate.lch.hue, taken) >= OTHER_GAP);
        if clear {
            others.push(candidate);
        }
    }
    let fallback = |(turn, chroma): (f32, f32)| ((hue + turn).rem_euclid(FULL_TURN), chroma);
    let (second_hue, second_chroma) =
        others.first().map_or(fallback(SECOND_FALLBACK), |patch| (patch.lch.hue, patch.lch.chroma));
    let (third_hue, third_chroma) =
        others.get(1).map_or(fallback(THIRD_FALLBACK), |patch| (patch.lch.hue, patch.lch.chroma));
    let pick =
        |grey_chroma: f32, chroma: f32, cap: f32| if grey { grey_chroma } else { chroma.min(cap) };
    let tone = |(light, chroma): (f32, f32)| oklch(light, chroma, hue);
    let base = tone(spec.base);
    let accent_light = if dark {
        seed.lch.light.max(DARK_ACCENT_FLOOR)
    } else {
        seed.lch.light.min(LIGHT_ACCENT_CEILING)
    };
    let at =
        |light: f32, chroma: f32, hue: f32| fit(Oklch { light, chroma, hue }, base, TARGET, dark);
    let mut made = StudioPalette {
        base,
        panel: tone(spec.panel),
        raised: tone(spec.raised),
        text: tone(spec.text),
        quiet: tone(spec.quiet),
        accent: at(accent_light, pick(GREY_CHROMA, seed.lch.chroma, ACCENT_CHROMA), hue),
        on_accent: tone(spec.on_accent),
        second: at(spec.second, pick(GREY_CHROMA, second_chroma, SECOND_CHROMA), second_hue),
        third: at(spec.third, pick(GREY_CHROMA, third_chroma, THIRD_CHROMA), third_hue),
        ansi: [base; ANSI_SLOTS],
    };
    polish(&mut made);
    let pulls: Vec<Pull> = colourful
        .iter()
        .map(|patch| Pull { hue: patch.lch.hue, strength: patch.share * patch.lch.chroma })
        .collect();
    made.ansi = terminal(&made, &pulls, &WALLPAPER_TERMINAL, grey, dark);
    Some(made)
}

#[cfg(test)]
#[path = "area_tests.rs"]
mod tests;
