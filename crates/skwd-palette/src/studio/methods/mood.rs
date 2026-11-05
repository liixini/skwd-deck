use super::super::analysis::{Analysis, Cluster};
use super::super::oklch::{FULL_TURN, oklch};
use super::super::polish::WALLPAPER_TERMINAL;
use super::{Draft, Made, Pair, Shape, coloured, finish, mean_warmth, theory_neutrals, warmth};

const MID_LIGHT: f32 = 0.5;
const LIGHT_SCALE: f32 = 2.2;
const SPREAD_SCALE: f32 = 2.5;
const CHROMA_SCALE: f32 = 4.0;
const SOFT_OFFSET: f32 = 0.45;
const SHARE_POWER: f32 = 0.4;
const CHROMA_TOP: f32 = 0.17;
const CHROMA_DROP: f32 = 0.1;
const GREY_SCALE: f32 = 0.15;
const NEUTRAL_FROM: f32 = 0.01;
const NEUTRAL_RISE: f32 = 0.02;
const PARTNER_TURN: f32 = 40.0;
const SECOND_SCALE: f32 = 0.7;
const THIRD_SCALE: f32 = 0.6;

struct Surface {
    base: f32,
    panel: f32,
    raised: f32,
    rise: f32,
    chroma: f32,
    chroma_rise: f32,
}

const DARK_SURFACE: Surface =
    Surface { base: 0.14, panel: 0.18, raised: 0.23, rise: 0.08, chroma: 0.01, chroma_rise: 0.02 };
const LIGHT_SURFACE: Surface = Surface {
    base: 0.94,
    panel: 0.91,
    raised: 0.87,
    rise: 0.035,
    chroma: 0.008,
    chroma_rise: 0.015,
};

struct Ink {
    base: Pair,
    rise: f32,
}

const ACCENT: Ink = Ink { base: Pair { dark: 0.7, light: 0.5 }, rise: 0.14 };
const SECOND: Ink = Ink { base: Pair { dark: 0.76, light: 0.48 }, rise: 0.1 };
const THIRD: Ink = Ink { base: Pair { dark: 0.8, light: 0.46 }, rise: 0.08 };

fn softness(analysis: &Analysis) -> f32 {
    let count = analysis.lab.len().max(1) as f32;
    let mean_light = analysis.lab.iter().map(|pixel| pixel[0]).sum::<f32>() / count;
    let spread = (analysis.lab.iter().map(|pixel| (pixel[0] - mean_light).powi(2)).sum::<f32>()
        / count)
        .sqrt();
    let mean_chroma =
        analysis.lab.iter().map(|pixel| pixel[1].hypot(pixel[2])).sum::<f32>() / count;
    ((mean_light - MID_LIGHT) * LIGHT_SCALE - spread * SPREAD_SCALE - mean_chroma * CHROMA_SCALE
        + SOFT_OFFSET)
        .clamp(-1.0, 1.0)
}

fn ink(spec: &Ink, soft: f32, dark: bool) -> f32 {
    if dark { spec.base.dark + spec.rise * soft } else { spec.base.light }
}

pub(super) fn make(analysis: &Analysis, dark: bool) -> Made {
    let found = coloured(analysis);
    let cool = -mean_warmth(&found) > 0.0;
    let side: Vec<Cluster> =
        found.iter().copied().filter(|cluster| (warmth(cluster.lch.hue) < 0.0) == cool).collect();
    let side = if side.is_empty() { found } else { side };
    let score = |cluster: &Cluster| cluster.lch.chroma * cluster.share.powf(SHARE_POWER);
    let accent = side
        .iter()
        .copied()
        .max_by(|a, b| score(a).total_cmp(&score(b)))
        .unwrap_or_else(|| analysis.seed());
    let soft = f32::midpoint(softness(analysis), 1.0);
    let chroma = (CHROMA_TOP - CHROMA_DROP * soft) * if analysis.grey { GREY_SCALE } else { 1.0 };
    let hue = accent.lch.hue;
    let mut neutrals = theory_neutrals(hue, NEUTRAL_FROM + NEUTRAL_RISE * soft, dark);
    let surface = if dark { &DARK_SURFACE } else { &LIGHT_SURFACE };
    let surface_chroma = surface.chroma + surface.chroma_rise * soft;
    for (slot, light) in [surface.base, surface.panel, surface.raised].into_iter().enumerate() {
        neutrals[slot] = oklch(light + surface.rise * soft, surface_chroma, hue);
    }
    let draft = Draft {
        neutrals,
        accent: oklch(ink(&ACCENT, soft, dark), chroma, hue),
        on_accent: neutrals[0],
        second: oklch(
            ink(&SECOND, soft, dark),
            chroma * SECOND_SCALE,
            (hue + PARTNER_TURN).rem_euclid(FULL_TURN),
        ),
        third: oklch(
            ink(&THIRD, soft, dark),
            chroma * THIRD_SCALE,
            (hue - PARTNER_TURN).rem_euclid(FULL_TURN),
        ),
    };
    Made {
        palette: finish(&draft, analysis, dark, &WALLPAPER_TERMINAL),
        mark: None,
        shape: Shape::Plain,
    }
}
