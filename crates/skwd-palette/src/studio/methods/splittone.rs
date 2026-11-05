use super::super::analysis::Analysis;
use super::super::oklch::{LAB_AXES, Lab, Oklch, lab_to_lch, oklch};
use super::super::palette::Role;
use super::super::polish::WALLPAPER_TERMINAL;
use super::{Draft, Made, Pair, Shape, finish, theory_neutrals};

const SHADOW_SHARE: f32 = 0.25;
const HIGHLIGHT_FROM: f32 = 0.85;
const MID_RANGE: (f32, f32) = (0.4, 0.6);
const SHADOW_CEILING: f32 = 0.45;
const HIGHLIGHT_FLOOR: f32 = 0.55;
const CHROMA_FLOOR: f32 = 0.02;
const TINT_SCALE: f32 = 1.6;
const TINT_CHROMA: f32 = 0.05;
const FLAT_TINT: f32 = 0.004;
const TEXT: Pair = Pair { dark: 0.93, light: 0.22 };
const TEXT_CHROMA: f32 = 0.035;
const FLAT_TEXT: f32 = 0.006;
const ACCENT: Pair = Pair { dark: 0.82, light: 0.5 };
const ACCENT_SCALE: f32 = 2.5;
const ACCENT_CHROMA: (f32, f32) = (0.09, 0.15);
const FLAT_ACCENT: f32 = 0.012;
const SECOND: Pair = Pair { dark: 0.74, light: 0.48 };
const SECOND_SCALE: f32 = 2.5;
const SECOND_CHROMA: (f32, f32) = (0.08, 0.14);
const THIRD: Pair = Pair { dark: 0.78, light: 0.46 };
const THIRD_SCALE: f32 = 2.0;
const THIRD_CHROMA: (f32, f32) = (0.06, 0.12);
const FLAT_PARTNER: f32 = 0.01;

fn mean(pixels: &[Lab]) -> Lab {
    let count = pixels.len().max(1) as f32;
    pixels
        .iter()
        .fold([0.0f32; LAB_AXES], |acc, pixel| {
            [acc[0] + pixel[0], acc[1] + pixel[1], acc[2] + pixel[2]]
        })
        .map(|channel| channel / count)
}

fn tinted(tone: Oklch) -> Option<Oklch> {
    (tone.chroma >= CHROMA_FLOOR).then_some(tone)
}

fn scaled(tone: Option<Oklch>, scale: f32, range: (f32, f32), flat: f32) -> f32 {
    tone.map_or(flat, |tone| (tone.chroma * scale).clamp(range.0, range.1))
}

pub(super) fn make(analysis: &Analysis, dark: bool) -> Made {
    let mut order = analysis.lab.clone();
    order.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let count = order.len();
    let at = |share: f32| (count as f32 * share) as usize;
    let mut shadows = mean(&order[..at(SHADOW_SHARE)]);
    let mut highlights = mean(&order[at(HIGHLIGHT_FROM)..]);
    let middle = mean(&order[at(MID_RANGE.0)..at(MID_RANGE.1)]);
    if shadows[0] > SHADOW_CEILING {
        shadows = [0.0; LAB_AXES];
    }
    if highlights[0] < HIGHLIGHT_FLOOR {
        highlights = [0.0; LAB_AXES];
    }
    let (shadow, highlight, mid) =
        (lab_to_lch(shadows), lab_to_lch(highlights), lab_to_lch(middle));
    let (shadow_tint, highlight_tint, mid_tint) = (tinted(shadow), tinted(highlight), tinted(mid));
    let tint = shadow_tint.map_or(FLAT_TINT, |tone| (tone.chroma * TINT_SCALE).min(TINT_CHROMA));
    let mut neutrals = theory_neutrals(shadow.hue, tint, dark);
    neutrals[Role::Text.index()] = oklch(
        TEXT.pick(dark),
        highlight_tint.map_or(FLAT_TEXT, |tone| tone.chroma.min(TEXT_CHROMA)),
        highlight.hue,
    );
    let lead = highlight_tint.or(mid_tint).or(shadow_tint);
    let lead_chroma = lead.filter(|tone| tone.chroma > 0.0);
    let draft = Draft {
        neutrals,
        accent: oklch(
            ACCENT.pick(dark),
            scaled(lead_chroma, ACCENT_SCALE, ACCENT_CHROMA, FLAT_ACCENT),
            lead.map_or(0.0, |tone| tone.hue),
        ),
        on_accent: neutrals[0],
        second: oklch(
            SECOND.pick(dark),
            scaled(shadow_tint, SECOND_SCALE, SECOND_CHROMA, FLAT_PARTNER),
            shadow.hue,
        ),
        third: oklch(
            THIRD.pick(dark),
            scaled(mid_tint, THIRD_SCALE, THIRD_CHROMA, FLAT_PARTNER),
            mid.hue,
        ),
    };
    Made {
        palette: finish(&draft, analysis, dark, &WALLPAPER_TERMINAL),
        mark: None,
        shape: Shape::Plain,
    }
}
