use super::super::analysis::{Analysis, Area};
use super::super::oklch::{hue_distance, lab_to_lch, oklch};
use super::super::polish::WALLPAPER_TERMINAL;
use super::{Draft, Lift, Made, Pair, Shape, Tone, finish};

const TOP: Area = Area { left: 0.0, top: 0.0, right: 1.0, bottom: 0.06 };
const MIDDLE: Area = Area { left: 0.2, top: 0.15, right: 0.8, bottom: 0.85 };
const BOTTOM: Area = Area { left: 0.0, top: 0.9, right: 1.0, bottom: 1.0 };
const GREY_CHROMA: f32 = 0.02;
const ACCENT_CHROMA: (f32, f32) = (0.08, 0.16);
const HALF_TURN: f32 = 180.0;
const RANK_CHROMA: f32 = 3.0;
const NEUTRAL_SCALE: f32 = 0.5;
const NEUTRAL_CHROMA: f32 = 0.03;
const STRIP_CHROMA: f32 = 0.09;
const LIGHT_CHROMA_SCALE: f32 = 0.6;
const DARK_BASE_SCALE: f32 = 0.55;
const DARK_BASE: (f32, f32) = (0.14, 0.22);
const LIGHT_BASE_FROM: f32 = 0.9;
const LIGHT_BASE_SCALE: f32 = 0.08;
const LIGHT_BASE: (f32, f32) = (0.93, 0.975);
const DARK_PANEL_STEP: f32 = 0.04;
const DARK_RAISED_STEP: f32 = 0.09;
const LIGHT_PANEL: f32 = 0.94;
const LIGHT_RAISED: f32 = 0.9;
const TEXT: Pair = Pair { dark: 0.93, light: 0.2 };
const TEXT_CHROMA: f32 = 0.012;
const QUIET: Pair = Pair { dark: 0.65, light: 0.5 };
const ACCENT: Pair = Pair { dark: 0.75, light: 0.52 };
const SECOND: Pair = Pair { dark: 0.78, light: 0.5 };
const THIRD: Pair = Pair { dark: 0.82, light: 0.5 };
const TARGET: Pair = Pair { dark: 6.0, light: 4.5 };
const ON_DARK: Tone = Tone { light: 0.2, chroma: 0.04 };
const ON_LIGHT: Tone = Tone { light: 0.98, chroma: 0.01 };

pub(super) fn make(analysis: &Analysis, dark: bool) -> Made {
    let top = lab_to_lch(analysis.region(TOP));
    let middle = lab_to_lch(analysis.region(MIDDLE));
    let bottom = lab_to_lch(analysis.region(BOTTOM));
    let rank = |found: &super::super::analysis::Cluster| {
        hue_distance(found.lch.hue, top.hue) / HALF_TURN
            + (found.lch.light - top.light).abs()
            + found.lch.chroma * RANK_CHROMA
    };
    let accent = analysis
        .chromatic()
        .copied()
        .max_by(|a, b| rank(a).total_cmp(&rank(b)))
        .unwrap_or_else(|| analysis.seed());
    let hue = middle.hue;
    let chroma = (middle.chroma * NEUTRAL_SCALE).min(NEUTRAL_CHROMA);
    let (base_light, panel_light, raised_light, surface_chroma) = if dark {
        let light = (middle.light * DARK_BASE_SCALE).clamp(DARK_BASE.0, DARK_BASE.1);
        (light, light + DARK_PANEL_STEP, light + DARK_RAISED_STEP, chroma)
    } else {
        let light =
            (LIGHT_BASE_FROM + middle.light * LIGHT_BASE_SCALE).clamp(LIGHT_BASE.0, LIGHT_BASE.1);
        (light, LIGHT_PANEL, LIGHT_RAISED, chroma * LIGHT_CHROMA_SCALE)
    };
    let base = oklch(base_light, surface_chroma, hue);
    let accent_chroma = if analysis.grey {
        GREY_CHROMA
    } else {
        accent.lch.chroma.clamp(ACCENT_CHROMA.0, ACCENT_CHROMA.1)
    };
    let target = TARGET.pick(dark);
    let lift = |light: Pair| Lift { light: light.pick(dark), chroma: 0.0, target };
    let draft = Draft {
        neutrals: [
            base,
            oklch(panel_light, surface_chroma, hue),
            oklch(raised_light, surface_chroma, hue),
            oklch(TEXT.pick(dark), TEXT_CHROMA, hue),
            oklch(QUIET.pick(dark), chroma, hue),
        ],
        accent: lift(ACCENT).fit(accent_chroma, accent.lch.hue, base, dark),
        on_accent: if dark { ON_DARK } else { ON_LIGHT }.at(accent.lch.hue),
        second: lift(SECOND).fit(bottom.chroma.min(STRIP_CHROMA), bottom.hue, base, dark),
        third: lift(THIRD).fit(top.chroma.min(STRIP_CHROMA), top.hue, base, dark),
    };
    Made {
        palette: finish(&draft, analysis, dark, &WALLPAPER_TERMINAL),
        mark: None,
        shape: Shape::Plain,
    }
}
