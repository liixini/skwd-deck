use super::super::analysis::{Analysis, Cluster};
use super::super::oklch::{FULL_TURN, oklch};
use super::super::polish::WALLPAPER_TERMINAL;
use super::{
    Draft, Made, Pair, Shape, coloured, finish, mean_warmth, strength, theory_neutrals, warmth,
};

const OTHER_SHARE: f32 = 0.005;
const OTHER_POWER: f32 = 0.3;
const OPPOSITE_CHROMA: (f32, f32) = (0.1, 0.17);
const FALLBACK_CHROMA: f32 = 0.14;
const HALF_TURN: f32 = 180.0;
const GREY_SCALE: f32 = 0.02;
const NEUTRAL_CHROMA: f32 = 0.018;
const GREY_NEUTRAL: f32 = 0.005;
const SECOND_CHROMA: f32 = 0.1;
const THIRD_CHROMA: f32 = 0.06;
const THIRD_TURN: f32 = 25.0;
const ACCENT: Pair = Pair { dark: 0.78, light: 0.52 };
const SECOND: Pair = Pair { dark: 0.78, light: 0.5 };
const THIRD: Pair = Pair { dark: 0.84, light: 0.45 };

fn is_warm(found: &Cluster) -> bool {
    warmth(found.lch.hue) >= 0.0
}

pub(super) fn make(analysis: &Analysis, dark: bool) -> Made {
    let found = coloured(analysis);
    let warm = mean_warmth(&found) >= 0.0;
    let dominant = found
        .iter()
        .copied()
        .filter(|cluster| is_warm(cluster) == warm)
        .max_by(|a, b| strength(a).total_cmp(&strength(b)))
        .unwrap_or_else(|| analysis.seed());
    let opposite_score = |cluster: &Cluster| cluster.lch.chroma * cluster.share.powf(OTHER_POWER);
    let opposite = found
        .iter()
        .copied()
        .filter(|cluster| is_warm(cluster) != warm && cluster.share > OTHER_SHARE)
        .max_by(|a, b| opposite_score(a).total_cmp(&opposite_score(b)));
    let (opposite_hue, opposite_chroma) = opposite.map_or(
        ((dominant.lch.hue + HALF_TURN).rem_euclid(FULL_TURN), FALLBACK_CHROMA),
        |cluster| (cluster.lch.hue, cluster.lch.chroma.clamp(OPPOSITE_CHROMA.0, OPPOSITE_CHROMA.1)),
    );
    let scale = if analysis.grey { GREY_SCALE } else { 1.0 };
    let neutral = if analysis.grey { GREY_NEUTRAL } else { NEUTRAL_CHROMA };
    let neutrals = theory_neutrals(dominant.lch.hue, neutral, dark);
    let turn = if warm { THIRD_TURN } else { -THIRD_TURN };
    let draft = Draft {
        neutrals,
        accent: oklch(ACCENT.pick(dark), opposite_chroma * scale, opposite_hue),
        on_accent: neutrals[0],
        second: oklch(
            SECOND.pick(dark),
            dominant.lch.chroma.min(SECOND_CHROMA) * scale,
            dominant.lch.hue,
        ),
        third: oklch(
            THIRD.pick(dark),
            THIRD_CHROMA * scale,
            (dominant.lch.hue + turn).rem_euclid(FULL_TURN),
        ),
    };
    Made {
        palette: finish(&draft, analysis, dark, &WALLPAPER_TERMINAL),
        mark: None,
        shape: Shape::Plain,
    }
}
