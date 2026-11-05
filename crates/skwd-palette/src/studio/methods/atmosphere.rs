use super::super::analysis::{Analysis, Cluster};
use super::super::oklch::{FULL_TURN, Oklch, lab_to_lch, oklch, to_oklch};
use super::super::polish::{Terminal, fit};
use super::{Draft, Made, Shape, finish};

const SOURCE_SHARE: f32 = 0.01;
const SOURCE_CHROMA_FLOOR: f32 = 0.03;
const GREY_CHROMA: f32 = 0.02;
const ACCENT_CHROMA: (f32, f32) = (0.06, 0.15);
const THIRD_CHROMA: f32 = 0.06;
const THIRD_TURN: f32 = 25.0;
const SECOND_SCALE: f32 = 1.4;
const SECOND_CHROMA: f32 = 0.08;
const TERMINAL: Terminal = Terminal { chroma: 0.09, pull: 0.8 };

struct Dark {
    base_scale: f32,
    base: (f32, f32),
    surface_scale: f32,
    surface_chroma: f32,
    panel_step: f32,
    raised_step: f32,
    raised_scale: f32,
    raised_chroma: f32,
    text: f32,
    text_scale: f32,
    text_chroma: f32,
    text_target: f32,
    quiet_step: f32,
    quiet_chroma: f32,
    accent_floor: f32,
    accent_target: f32,
    second: f32,
    second_target: f32,
    third: f32,
    third_target: f32,
}

struct Light {
    base: f32,
    panel: f32,
    raised: f32,
    surface_scale: f32,
    surface_chroma: f32,
    text: f32,
    text_chroma: f32,
    quiet: f32,
    quiet_chroma: f32,
    accent: f32,
    second: f32,
    third: f32,
    target: f32,
}

const DARK: Dark = Dark {
    base_scale: 0.7,
    base: (0.2, 0.3),
    surface_scale: 0.9,
    surface_chroma: 0.06,
    panel_step: 0.035,
    raised_step: 0.08,
    raised_scale: 0.8,
    raised_chroma: 0.055,
    text: 0.9,
    text_scale: 0.4,
    text_chroma: 0.04,
    text_target: 7.0,
    quiet_step: 0.38,
    quiet_chroma: 0.05,
    accent_floor: 0.78,
    accent_target: 6.0,
    second: 0.74,
    second_target: 4.5,
    third: 0.8,
    third_target: 6.0,
};

const LIGHT: Light = Light {
    base: 0.9,
    panel: 0.87,
    raised: 0.82,
    surface_scale: 0.7,
    surface_chroma: 0.045,
    text: 0.24,
    text_chroma: 0.04,
    quiet: 0.48,
    quiet_chroma: 0.05,
    accent: 0.5,
    second: 0.48,
    third: 0.48,
    target: 4.5,
};

fn source(analysis: &Analysis) -> Cluster {
    let score =
        |found: &Cluster| found.lch.light.powi(2) * (found.lch.chroma + SOURCE_CHROMA_FLOOR);
    analysis
        .clusters
        .iter()
        .copied()
        .filter(|found| found.share > SOURCE_SHARE)
        .max_by(|a, b| score(a).total_cmp(&score(b)))
        .unwrap_or_else(|| analysis.largest())
}

pub(super) fn make(analysis: &Analysis, dark: bool) -> Made {
    let mean = lab_to_lch(analysis.mean);
    let (light, chroma, hue) = (mean.light, mean.chroma, mean.hue);
    let src = source(analysis);
    let accent_chroma = if analysis.grey {
        GREY_CHROMA
    } else {
        src.lch.chroma.clamp(ACCENT_CHROMA.0, ACCENT_CHROMA.1)
    };
    let third_chroma = if analysis.grey { GREY_CHROMA } else { THIRD_CHROMA };
    let third_hue = (src.lch.hue + THIRD_TURN).rem_euclid(FULL_TURN);
    let second_chroma = (chroma * SECOND_SCALE).min(SECOND_CHROMA);
    let at = |light: f32, chroma: f32, hue: f32| Oklch { light, chroma, hue };
    let draft = if dark {
        let spec = &DARK;
        let surface = (chroma * spec.surface_scale).min(spec.surface_chroma);
        let base = oklch((light * spec.base_scale).clamp(spec.base.0, spec.base.1), surface, hue);
        let base_light = to_oklch(base).light;
        Draft {
            neutrals: [
                base,
                oklch(base_light + spec.panel_step, surface, hue),
                oklch(
                    base_light + spec.raised_step,
                    (chroma * spec.raised_scale).min(spec.raised_chroma),
                    hue,
                ),
                fit(
                    at(
                        spec.text,
                        (src.lch.chroma * spec.text_scale).min(spec.text_chroma),
                        src.lch.hue,
                    ),
                    base,
                    spec.text_target,
                    dark,
                ),
                oklch(base_light + spec.quiet_step, chroma.min(spec.quiet_chroma), hue),
            ],
            accent: fit(
                at(src.lch.light.max(spec.accent_floor), accent_chroma, src.lch.hue),
                base,
                spec.accent_target,
                dark,
            ),
            on_accent: base,
            second: fit(at(spec.second, second_chroma, hue), base, spec.second_target, dark),
            third: fit(at(spec.third, third_chroma, third_hue), base, spec.third_target, dark),
        }
    } else {
        let spec = &LIGHT;
        let surface = (chroma * spec.surface_scale).min(spec.surface_chroma);
        let base = oklch(spec.base, surface, hue);
        Draft {
            neutrals: [
                base,
                oklch(spec.panel, surface, hue),
                oklch(spec.raised, surface, hue),
                oklch(spec.text, chroma.min(spec.text_chroma), hue),
                oklch(spec.quiet, chroma.min(spec.quiet_chroma), hue),
            ],
            accent: fit(at(spec.accent, accent_chroma, src.lch.hue), base, spec.target, dark),
            on_accent: base,
            second: fit(at(spec.second, second_chroma, hue), base, spec.target, dark),
            third: fit(at(spec.third, third_chroma, third_hue), base, spec.target, dark),
        }
    };
    Made {
        palette: finish(&draft, analysis, dark, &TERMINAL),
        mark: Some([src.x, src.y]),
        shape: Shape::Plain,
    }
}
