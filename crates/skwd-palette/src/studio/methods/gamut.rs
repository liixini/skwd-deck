use super::super::analysis::Analysis;
use super::super::oklch::{FULL_TURN, hue_distance, oklch};
use super::super::polish::WALLPAPER_TERMINAL;
use super::{Draft, Made, Pair, Shape, finish, grey_theory, strength, theory_neutrals};

const BINS: usize = 72;
const BIN_DEGREES: f32 = 5.0;
const COVERAGE: f32 = 0.85;
const DISTINCT: f32 = 15.0;
const REACH_CHROMA: f32 = 0.15;
const HALF_TURN: f32 = 180.0;
const NEUTRAL_CHROMA: f32 = 0.012;
const ACCENT: Pair = Pair { dark: 0.78, light: 0.5 };
const ACCENT_CHROMA: f32 = 0.15;
const SECOND_TWO: Pair = Pair { dark: 0.72, light: 0.48 };
const SECOND_ONE: Pair = Pair { dark: 0.86, light: 0.38 };
const SECOND_TWO_CHROMA: f32 = 0.13;
const SECOND_ONE_CHROMA: f32 = 0.07;
const THIRD: Pair = Pair { dark: 0.8, light: 0.45 };
const THIRD_SHARE: f32 = 0.35;
const NARROWEST: f32 = 30.0;
const WIDEST: f32 = 150.0;
const RIM_STEP: f32 = 15.0;

fn centre(bin: usize) -> f32 {
    (bin as f32 + 0.5) * BIN_DEGREES
}

fn heaviest(bins: &[f32; BINS], candidates: &[usize]) -> Option<usize> {
    candidates.iter().copied().fold(None, |best, bin| match best {
        Some(kept) if bins[kept] >= bins[bin] => Some(kept),
        _ => Some(bin),
    })
}

fn mask(low: f32, span: f32, middle: f32, opposite: f32, reach: f32) -> Vec<[f32; 2]> {
    let (low, span) =
        if span < NARROWEST { (middle - NARROWEST / 2.0, NARROWEST) } else { (low, span) };
    if span <= WIDEST {
        return vec![
            [low.rem_euclid(FULL_TURN), reach],
            [(low + span).rem_euclid(FULL_TURN), reach],
            [opposite, THIRD_SHARE * reach],
        ];
    }
    let count = (span / RIM_STEP).floor() as usize + 2;
    let step = span / (count - 1) as f32;
    let mut rim: Vec<[f32; 2]> = (0..count)
        .map(|index| [(low + index as f32 * step).rem_euclid(FULL_TURN), reach])
        .collect();
    rim.push([opposite, 0.0]);
    rim
}

pub(super) fn make(analysis: &Analysis, dark: bool) -> Made {
    if analysis.grey || analysis.chromatic().next().is_none() {
        return grey_theory(analysis, dark);
    }
    let mut bins = [0.0f32; BINS];
    for found in analysis.chromatic() {
        bins[(found.lch.hue / BIN_DEGREES) as usize % BINS] += strength(found);
    }
    let total: f32 = bins.iter().sum();
    let mut best: Option<(usize, usize)> = None;
    for start in (0..BINS).filter(|bin| bins[*bin] > 0.0) {
        let mut covered = 0.0;
        for width in 0..BINS {
            covered += bins[(start + width) % BINS];
            if covered >= COVERAGE * total {
                if best.is_none_or(|(_, kept)| width < kept) {
                    best = Some((start, width));
                }
                break;
            }
        }
    }
    let (start, width) = best.unwrap_or((0, BINS - 1));
    let low = start as f32 * BIN_DEGREES;
    let span = (width + 1) as f32 * BIN_DEGREES;
    let occupied: Vec<usize> =
        (0..=width).map(|step| (start + step) % BINS).filter(|bin| bins[*bin] > 0.0).collect();
    let peak = heaviest(&bins, &occupied).unwrap_or(start);
    let peak_hue = centre(peak);
    let rest: Vec<usize> = occupied
        .iter()
        .copied()
        .filter(|bin| hue_distance(centre(*bin), peak_hue) >= DISTINCT)
        .collect();
    let second_hue = heaviest(&bins, &rest).map_or(peak_hue, centre);
    let strongest = analysis.chromatic().map(|found| found.lch.chroma).fold(0.0, f32::max);
    let reach = (strongest / REACH_CHROMA).min(1.0);
    let middle = (low + span / 2.0).rem_euclid(FULL_TURN);
    let opposite = (middle + HALF_TURN).rem_euclid(FULL_TURN);
    let neutrals = theory_neutrals(middle, NEUTRAL_CHROMA, dark);
    let two = hue_distance(second_hue, peak_hue) >= DISTINCT;
    let (second_light, second_chroma) =
        if two { (SECOND_TWO, SECOND_TWO_CHROMA) } else { (SECOND_ONE, SECOND_ONE_CHROMA) };
    let draft = Draft {
        neutrals,
        accent: oklch(ACCENT.pick(dark), ACCENT_CHROMA * reach, peak_hue),
        on_accent: neutrals[0],
        second: oklch(second_light.pick(dark), second_chroma * reach, second_hue),
        third: oklch(THIRD.pick(dark), ACCENT_CHROMA * THIRD_SHARE * reach, opposite),
    };
    Made {
        palette: finish(&draft, analysis, dark, &WALLPAPER_TERMINAL),
        mark: None,
        shape: Shape::Mask(mask(low, span, middle, opposite, reach)),
    }
}
