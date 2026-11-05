use crate::Rgb;
use crate::semantic::contrast;

use super::oklch::{FULL_TURN, Oklch, from_oklch, hue_distance, hue_offset, oklch, to_oklch};
use super::palette::{ANSI_SLOTS, StudioPalette};

pub const TEXT_CONTRAST: f32 = 7.0;
pub const READABLE_CONTRAST: f32 = 4.5;
const LIGHT_STEP: f32 = 0.01;
const FIT_STEPS: usize = 80;
const FIT_LIGHT: (f32, f32) = (0.02, 0.99);
const ENSURE_STEPS: usize = 100;
const ENSURE_LIGHT: (f32, f32) = (0.005, 0.995);
pub(crate) const DARK_SURFACE_LIGHT: f32 = 0.6;
const WHITE: Rgb = Rgb(u8::MAX, u8::MAX, u8::MAX);
const BLACK: Rgb = Rgb(0, 0, 0);

pub fn fit(color: Oklch, against: Rgb, target: f32, dark: bool) -> Rgb {
    let step = if dark { LIGHT_STEP } else { -LIGHT_STEP };
    let mut light = color.light;
    for _ in 0..FIT_STEPS {
        let candidate = oklch(light, color.chroma, color.hue);
        if contrast(candidate, against) >= target {
            return candidate;
        }
        light = (light + step).clamp(FIT_LIGHT.0, FIT_LIGHT.1);
    }
    oklch(light, color.chroma, color.hue)
}

pub fn ensure(color: Rgb, against: Rgb, target: f32) -> Rgb {
    if contrast(color, against) >= target {
        return color;
    }
    let start = to_oklch(color);
    let up = to_oklch(against).light < DARK_SURFACE_LIGHT;
    let mut light = start.light;
    for _ in 0..ENSURE_STEPS {
        light = (light + if up { LIGHT_STEP } else { -LIGHT_STEP })
            .clamp(ENSURE_LIGHT.0, ENSURE_LIGHT.1);
        let candidate = from_oklch(Oklch { light, ..start });
        if contrast(candidate, against) >= target {
            return candidate;
        }
    }
    if contrast(WHITE, against) >= contrast(BLACK, against) { WHITE } else { BLACK }
}

pub fn polish(palette: &mut StudioPalette) {
    palette.text = ensure(palette.text, palette.raised, TEXT_CONTRAST);
    palette.quiet = ensure(palette.quiet, palette.raised, READABLE_CONTRAST);
    palette.accent = ensure(
        ensure(palette.accent, palette.panel, READABLE_CONTRAST),
        palette.base,
        READABLE_CONTRAST,
    );
    palette.second = ensure(palette.second, palette.raised, READABLE_CONTRAST);
    palette.third = ensure(palette.third, palette.raised, READABLE_CONTRAST);
    palette.on_accent = ensure(palette.on_accent, palette.accent, READABLE_CONTRAST);
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pull {
    pub hue: f32,
    pub strength: f32,
}

struct Shade {
    dark: f32,
    light: f32,
    chroma: f32,
    target: f32,
}

pub struct Terminal {
    pub chroma: f32,
    pub pull: f32,
}

pub const WALLPAPER_TERMINAL: Terminal = Terminal { chroma: 0.13, pull: 0.6 };
pub const THEME_TERMINAL: Terminal = Terminal { chroma: 0.11, pull: 0.5 };
const ANCHORS: [f32; 6] = [29.0, 142.0, 100.0, 255.0, 330.0, 195.0];
const GREY_CHROMA: f32 = 0.6;
const SNAP_DEGREES: f32 = 35.0;
const CAP_DIVISOR: f32 = 4.0;
const WHITE_MIX: f32 = 0.12;
const NORMAL: Shade = Shade { dark: 0.72, light: 0.50, chroma: 1.0, target: READABLE_CONTRAST };
const BRIGHT: Shade = Shade { dark: 0.82, light: 0.42, chroma: 1.1, target: 6.0 };

fn anchor_cap(anchor: f32) -> f32 {
    let mut ring = ANCHORS;
    ring.sort_by(f32::total_cmp);
    let index = ring.iter().position(|hue| *hue == anchor).unwrap_or(0);
    let before = ring[(index + ring.len() - 1) % ring.len()];
    let after = ring[(index + 1) % ring.len()];
    hue_distance(anchor, before).min(hue_distance(anchor, after)) / CAP_DIVISOR
}

fn anchored(anchor: f32, pulls: &[Pull], pull: f32) -> f32 {
    let Some(best) = pulls
        .iter()
        .filter(|pull| hue_distance(pull.hue, anchor) <= SNAP_DEGREES)
        .max_by(|a, b| a.strength.total_cmp(&b.strength))
    else {
        return anchor;
    };
    let cap = anchor_cap(anchor);
    (anchor + (hue_offset(anchor, best.hue) * pull).clamp(-cap, cap)).rem_euclid(FULL_TURN)
}

pub fn terminal(
    palette: &StudioPalette,
    pulls: &[Pull],
    spec: &Terminal,
    grey: bool,
    dark: bool,
) -> [Rgb; ANSI_SLOTS] {
    let chroma = if grey { spec.chroma * GREY_CHROMA } else { spec.chroma };
    let tone = |shade: &Shade, hue: f32| {
        let light = if dark { shade.dark } else { shade.light };
        fit(Oklch { light, chroma: chroma * shade.chroma, hue }, palette.base, shade.target, dark)
    };
    let hues = ANCHORS.map(|anchor| anchored(anchor, pulls, spec.pull));
    let white = if dark { palette.text.lerp(palette.base, WHITE_MIX) } else { palette.text };
    let mut slots = [palette.raised; ANSI_SLOTS];
    for (index, hue) in hues.iter().enumerate() {
        slots[1 + index] = tone(&NORMAL, *hue);
        slots[ANSI_SLOTS / 2 + 1 + index] = tone(&BRIGHT, *hue);
    }
    slots[ANSI_SLOTS / 2 - 1] = white;
    slots[ANSI_SLOTS / 2] = palette.quiet;
    slots[ANSI_SLOTS - 1] = palette.text;
    slots
}

#[cfg(test)]
#[path = "polish_tests.rs"]
mod tests;
