use super::super::analysis::Analysis;
use super::super::oklch::FULL_TURN;
use super::super::polish::WALLPAPER_TERMINAL;
use super::{Draft, Lift, Made, Shape, Tone, finish, strength};

const NARROW_SPREAD: f32 = 35.0;
const SPLIT: (f32, f32) = (150.0, 210.0);
const TRIAD: (f32, f32) = (120.0, 240.0);
const ACCENT_CHROMA: (f32, f32) = (0.08, 0.14);
const R_FLOOR: f32 = 1e-6;
const SPREAD_SCALE: f32 = -2.0;

struct Spec {
    base: Tone,
    panel: Tone,
    raised: Tone,
    text: Tone,
    quiet: Tone,
    accent: f32,
    partner_chroma: f32,
    target: f32,
    on_accent: Tone,
}

const DARK: Spec = Spec {
    base: Tone { light: 0.18, chroma: 0.02 },
    panel: Tone { light: 0.22, chroma: 0.022 },
    raised: Tone { light: 0.27, chroma: 0.024 },
    text: Tone { light: 0.93, chroma: 0.01 },
    quiet: Tone { light: 0.65, chroma: 0.025 },
    accent: 0.76,
    partner_chroma: 0.11,
    target: 6.0,
    on_accent: Tone { light: 0.2, chroma: 0.05 },
};

const LIGHT: Spec = Spec {
    base: Tone { light: 0.97, chroma: 0.01 },
    panel: Tone { light: 0.94, chroma: 0.014 },
    raised: Tone { light: 0.9, chroma: 0.018 },
    text: Tone { light: 0.22, chroma: 0.02 },
    quiet: Tone { light: 0.5, chroma: 0.025 },
    accent: 0.52,
    partner_chroma: 0.12,
    target: 4.5,
    on_accent: Tone { light: 0.98, chroma: 0.01 },
};

pub(super) fn spread(analysis: &Analysis) -> f32 {
    let weighted: Vec<(f32, f32)> =
        analysis.chromatic().map(|found| (strength(found), found.lch.hue.to_radians())).collect();
    let weighted = if weighted.is_empty() { vec![(1.0, 0.0)] } else { weighted };
    let (x, y, total) =
        weighted.iter().fold((0.0f32, 0.0f32, 0.0f32), |(x, y, total), (weight, angle)| {
            (x + weight * angle.cos(), y + weight * angle.sin(), total + weight)
        });
    let resultant = x.hypot(y) / total;
    (SPREAD_SCALE * resultant.max(R_FLOOR).ln()).sqrt().to_degrees()
}

pub(super) fn make(analysis: &Analysis, dark: bool) -> Made {
    let spec = if dark { &DARK } else { &LIGHT };
    let seed = analysis.seed();
    let hue = seed.lch.hue;
    let (second_turn, third_turn) = if spread(analysis) < NARROW_SPREAD { SPLIT } else { TRIAD };
    let base = spec.base.at(hue);
    let lift = Lift { light: spec.accent, chroma: 0.0, target: spec.target };
    let draft = Draft {
        neutrals: [
            base,
            spec.panel.at(hue),
            spec.raised.at(hue),
            spec.text.at(hue),
            spec.quiet.at(hue),
        ],
        accent: lift.fit(seed.lch.chroma.clamp(ACCENT_CHROMA.0, ACCENT_CHROMA.1), hue, base, dark),
        on_accent: spec.on_accent.at(hue),
        second: lift.fit(
            spec.partner_chroma,
            (hue + second_turn).rem_euclid(FULL_TURN),
            base,
            dark,
        ),
        third: lift.fit(spec.partner_chroma, (hue + third_turn).rem_euclid(FULL_TURN), base, dark),
    };
    Made {
        palette: finish(&draft, analysis, dark, &WALLPAPER_TERMINAL),
        mark: None,
        shape: Shape::Plain,
    }
}
