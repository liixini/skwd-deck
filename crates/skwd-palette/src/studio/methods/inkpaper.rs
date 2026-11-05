use super::super::analysis::{Analysis, Cluster};
use super::super::oklch::oklch;
use super::super::polish::Terminal;
use super::{Draft, Lift, Made, Pair, Shape, finish};

const SHARE_PULL: f32 = 0.3;
const PAPER_LIGHT: Pair = Pair { dark: 0.16, light: 0.955 };
const INK_LIGHT: Pair = Pair { dark: 0.91, light: 0.2 };
const PAPER_SCALE: f32 = 0.6;
const PAPER_CHROMA: f32 = 0.03;
const INK_CHROMA: f32 = 0.035;
const SPOT: Pair = Pair { dark: 0.7, light: 0.52 };
const SPOT_TARGET: f32 = 4.5;
const GREY_CHROMA: f32 = 0.02;
const SPOT_CHROMA: (f32, f32) = (0.1, 0.18);
const PANEL_MIX: f32 = 0.05;
const RAISED_MIX: f32 = 0.11;
const QUIET_MIX: f32 = 0.45;
const SECOND_MIX: f32 = 0.2;
const THIRD_MIX: f32 = 0.45;
const TERMINAL: Terminal = Terminal { chroma: 0.06, pull: 0.9 };

pub(super) fn make(analysis: &Analysis, dark: bool) -> Made {
    let darkest = |found: &Cluster| found.lch.light - found.share * SHARE_PULL;
    let lightest = |found: &Cluster| found.lch.light + found.share * SHARE_PULL;
    let deep = analysis
        .clusters
        .iter()
        .copied()
        .min_by(|a, b| darkest(a).total_cmp(&darkest(b)))
        .unwrap_or_else(|| analysis.seed());
    let pale = analysis
        .clusters
        .iter()
        .copied()
        .max_by(|a, b| lightest(a).total_cmp(&lightest(b)))
        .unwrap_or_else(|| analysis.seed());
    let (paper_from, ink_from) = if dark { (deep, pale) } else { (pale, deep) };
    let paper = oklch(
        PAPER_LIGHT.pick(dark),
        (paper_from.lch.chroma * PAPER_SCALE).min(PAPER_CHROMA),
        paper_from.lch.hue,
    );
    let ink = oklch(INK_LIGHT.pick(dark), ink_from.lch.chroma.min(INK_CHROMA), ink_from.lch.hue);
    let spot = analysis.seed();
    let spot_chroma = if analysis.grey {
        GREY_CHROMA
    } else {
        spot.lch.chroma.clamp(SPOT_CHROMA.0, SPOT_CHROMA.1)
    };
    let accent = Lift { light: SPOT.pick(dark), chroma: 0.0, target: SPOT_TARGET }.fit(
        spot_chroma,
        spot.lch.hue,
        paper,
        dark,
    );
    let draft = Draft {
        neutrals: [
            paper,
            paper.lerp(ink, PANEL_MIX),
            paper.lerp(ink, RAISED_MIX),
            ink,
            ink.lerp(paper, QUIET_MIX),
        ],
        accent,
        on_accent: paper,
        second: ink.lerp(paper, SECOND_MIX),
        third: accent.lerp(ink, THIRD_MIX),
    };
    Made { palette: finish(&draft, analysis, dark, &TERMINAL), mark: None, shape: Shape::Plain }
}
