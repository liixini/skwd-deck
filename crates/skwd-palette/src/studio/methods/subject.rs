use crate::Rgb;
use crate::semantic::contrast;

use super::super::analysis::{Analysis, Cluster};
use super::super::oklch::{FULL_TURN, hue_distance, oklch};
use super::super::polish::WALLPAPER_TERMINAL;
use super::{Draft, Lift, Made, Shape, Tone, finish};

struct Spec {
    base: f32,
    panel: f32,
    raised: f32,
    quiet: f32,
    surface_scale: f32,
    text: Tone,
    accent: Lift,
    second: Lift,
    third: Lift,
}

const GREY_CHROMA: f32 = 0.02;
const ACCENT_CHROMA: (f32, f32) = (0.10, 0.19);
const SECOND_CHROMA: f32 = 0.10;
const NEUTRAL_SCALE: f32 = 0.35;
const NEUTRAL_CHROMA: f32 = 0.02;
const RARITY_POWER: f32 = 0.5;
const CHROMA_FLOOR: f32 = 0.02;
const SECOND_GAP: f32 = 40.0;
const ON_ACCENT_DARK: Rgb = Rgb(0x11, 0x11, 0x11);
const ON_ACCENT_LIGHT: Rgb = Rgb(u8::MAX, u8::MAX, u8::MAX);
const ON_ACCENT_TONE: Tone = Tone { light: 0.18, chroma: 0.03 };

const DARK: Spec = Spec {
    base: 0.16,
    panel: 0.20,
    raised: 0.25,
    quiet: 0.64,
    surface_scale: 1.0,
    text: Tone { light: 0.93, chroma: 0.01 },
    accent: Lift { light: 0.68, chroma: 0.0, target: 5.0 },
    second: Lift { light: 0.76, chroma: SECOND_CHROMA, target: 6.0 },
    third: Lift { light: 0.78, chroma: 0.05, target: 6.0 },
};

const LIGHT: Spec = Spec {
    base: 0.975,
    panel: 0.945,
    raised: 0.905,
    quiet: 0.50,
    surface_scale: 0.6,
    text: Tone { light: 0.21, chroma: 0.012 },
    accent: Lift { light: 0.55, chroma: 0.0, target: 4.5 },
    second: Lift { light: 0.50, chroma: SECOND_CHROMA, target: 4.5 },
    third: Lift { light: 0.50, chroma: 0.05, target: 4.5 },
};

fn ranked(analysis: &Analysis, saliency: &[f32]) -> Vec<(f32, Cluster)> {
    let ids = analysis.clusters.iter().map(|found| found.id + 1).max().unwrap_or(0);
    let mut totals = vec![(0.0f32, 0usize); ids];
    for (owner, value) in analysis.owner.iter().zip(saliency) {
        if let Some(slot) = totals.get_mut(*owner) {
            slot.0 += value;
            slot.1 += 1;
        }
    }
    let mut scored: Vec<(f32, Cluster)> = analysis
        .chromatic()
        .map(|found| {
            let (sum, count) = totals.get(found.id).copied().unwrap_or((0.0, 1));
            let mean = sum / count.max(1) as f32;
            let score =
                mean * (1.0 - found.share).powf(RARITY_POWER) * (found.lch.chroma + CHROMA_FLOOR);
            (score, *found)
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    scored
}

fn focus(analysis: &Analysis, saliency: &[f32], id: usize) -> [f32; 2] {
    let (mut sum_x, mut sum_y, mut total) = (0.0f32, 0.0f32, 0.0f32);
    for (index, (owner, value)) in analysis.owner.iter().zip(saliency).enumerate() {
        if *owner == id {
            sum_x += (index % analysis.width) as f32 * value;
            sum_y += (index / analysis.width) as f32 * value;
            total += value;
        }
    }
    if total <= 0.0 {
        return [0.5, 0.5];
    }
    [sum_x / total / analysis.width as f32, sum_y / total / analysis.height as f32]
}

pub(super) fn make(analysis: &Analysis, dark: bool) -> Made {
    let spec = if dark { &DARK } else { &LIGHT };
    let saliency = analysis.saliency();
    let scores = ranked(analysis, &saliency);
    let mass = analysis.largest();
    let accent = scores.first().map_or_else(|| analysis.seed(), |(_, found)| *found);
    let second = scores
        .iter()
        .find(|(_, found)| hue_distance(found.lch.hue, accent.lch.hue) > SECOND_GAP)
        .map_or(mass, |(_, found)| *found);
    let neutral_hue = mass.lch.hue;
    let neutral_chroma = (mass.lch.chroma * NEUTRAL_SCALE).min(NEUTRAL_CHROMA);
    let surface = |light: f32| oklch(light, neutral_chroma * spec.surface_scale, neutral_hue);
    let base = surface(spec.base);
    let accent_chroma = if analysis.grey {
        GREY_CHROMA
    } else {
        accent.lch.chroma.clamp(ACCENT_CHROMA.0, ACCENT_CHROMA.1)
    };
    let third_chroma = if analysis.grey { GREY_CHROMA } else { spec.third.chroma };
    let accent_color = spec.accent.fit(accent_chroma, accent.lch.hue, base, dark);
    let on_accent =
        if contrast(accent_color, ON_ACCENT_LIGHT) >= contrast(accent_color, ON_ACCENT_DARK) {
            ON_ACCENT_LIGHT
        } else {
            ON_ACCENT_TONE.at(accent.lch.hue.rem_euclid(FULL_TURN))
        };
    let draft = Draft {
        neutrals: [
            base,
            surface(spec.panel),
            surface(spec.raised),
            oklch(spec.text.light, spec.text.chroma, neutral_hue),
            oklch(spec.quiet, neutral_chroma, neutral_hue),
        ],
        accent: accent_color,
        on_accent,
        second: spec.second.fit(second.lch.chroma.min(SECOND_CHROMA), second.lch.hue, base, dark),
        third: spec.third.fit(third_chroma, neutral_hue, base, dark),
    };
    let mark = (accent.id != usize::MAX).then(|| focus(analysis, &saliency, accent.id));
    Made { palette: finish(&draft, analysis, dark, &WALLPAPER_TERMINAL), mark, shape: Shape::Plain }
}
