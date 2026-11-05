use crate::Rgb;
use crate::semantic::contrast;

use super::super::analysis::{Analysis, Cluster};
use super::super::oklch::{hue_distance, oklch, to_oklch};
use super::super::polish::Terminal;
use super::{Draft, Made, Pair, Shape, Tone, finish};

const BIG_SHARE: f32 = 0.04;
const NEUTRAL_CHROMA: f32 = 0.05;
const BASE_LIGHT_DARK: (f32, f32) = (0.15, 0.23);
const BASE_LIGHT_LIGHT: (f32, f32) = (0.93, 0.975);
const LIGHT_CHROMA_SCALE: f32 = 0.6;
const PANEL_STEP: Pair = Pair { dark: 0.04, light: -0.03 };
const RAISED_STEP: Pair = Pair { dark: 0.09, light: -0.07 };
const RAISED_CHROMA_DARK: f32 = 0.9;
const RANK_CHROMA: f32 = 0.6;
const RANK_SHARE: f32 = 0.4;
const PICK_GAP: f32 = 35.0;
const PICKS: usize = 3;
const TEXT_LIGHT: Pair = Pair { dark: 0.93, light: 0.2 };
const TEXT_CHROMA: f32 = 0.03;
const QUIET_MIX: f32 = 0.45;
const ACCENT_RANGE_DARK: (f32, f32) = (0.58, 0.9);
const ACCENT_RANGE_LIGHT: (f32, f32) = (0.3, 0.6);
const ACCENT_CHROMA: f32 = 0.2;
const REPEAT_LIGHT_DARK: [f32; 2] = [0.86, 0.74];
const REPEAT_LIGHT_LIGHT: [f32; 2] = [0.3, 0.42];
const REPEAT_CHROMA: [f32; 2] = [0.35, 0.7];
const ACCENT_TARGET: f32 = 4.5;
const ON_DARK: Tone = Tone { light: 0.2, chroma: 0.04 };
const ON_LIGHT: Tone = Tone { light: 0.97, chroma: 0.01 };
const BLACK: Rgb = Rgb(0, 0, 0);
const WHITE: Rgb = Rgb(u8::MAX, u8::MAX, u8::MAX);
const TERMINAL: Terminal = Terminal { chroma: 0.15, pull: 0.85 };

fn picks(analysis: &Analysis) -> Vec<Cluster> {
    let mut ranked: Vec<Cluster> = analysis.chromatic().copied().collect();
    let rank = |found: &Cluster| found.lch.chroma * RANK_CHROMA + found.share * RANK_SHARE;
    ranked.sort_by(|a, b| rank(b).total_cmp(&rank(a)));
    let mut picked: Vec<Cluster> = Vec::new();
    for candidate in ranked {
        if picked.iter().all(|found| hue_distance(candidate.lch.hue, found.lch.hue) > PICK_GAP) {
            picked.push(candidate);
        }
    }
    if picked.is_empty() {
        picked.push(analysis.largest());
    }
    while picked.len() < PICKS {
        picked.push(picked[0]);
    }
    picked.truncate(PICKS);
    picked
}

pub(super) fn make(analysis: &Analysis, dark: bool) -> Made {
    let big: Vec<Cluster> =
        analysis.clusters.iter().copied().filter(|found| found.share > BIG_SHARE).collect();
    let pool = if big.is_empty() { analysis.clusters.clone() } else { big };
    let by_light = |a: &&Cluster, b: &&Cluster| a.lch.light.total_cmp(&b.lch.light);
    let base_cluster =
        if dark { pool.iter().min_by(by_light) } else { pool.iter().max_by(by_light) }
            .copied()
            .unwrap_or_else(|| analysis.seed());
    let hue = base_cluster.lch.hue;
    let chroma = base_cluster.lch.chroma.min(NEUTRAL_CHROMA);
    let (range, surface_chroma) = if dark {
        (BASE_LIGHT_DARK, chroma)
    } else {
        (BASE_LIGHT_LIGHT, chroma * LIGHT_CHROMA_SCALE)
    };
    let light = base_cluster.lch.light.clamp(range.0, range.1);
    let raised_chroma = if dark { chroma * RAISED_CHROMA_DARK } else { surface_chroma };
    let base = oklch(light, surface_chroma, hue);
    let panel = oklch(light + PANEL_STEP.pick(dark), surface_chroma, hue);
    let raised = oklch(light + RAISED_STEP.pick(dark), raised_chroma, hue);
    let picked = picks(analysis);
    let lit = analysis
        .clusters
        .iter()
        .copied()
        .max_by(|a, b| a.lch.light.total_cmp(&b.lch.light))
        .unwrap_or_else(|| analysis.seed());
    let text = oklch(TEXT_LIGHT.pick(dark), lit.lch.chroma.min(TEXT_CHROMA), lit.lch.hue);
    let (range, repeat_light) = if dark {
        (ACCENT_RANGE_DARK, REPEAT_LIGHT_DARK)
    } else {
        (ACCENT_RANGE_LIGHT, REPEAT_LIGHT_LIGHT)
    };
    let mut seen: Vec<usize> = Vec::new();
    let mut accents = Vec::with_capacity(PICKS);
    for found in &picked {
        let mut light = found.lch.light.clamp(range.0, range.1);
        let mut chroma = found.lch.chroma.min(ACCENT_CHROMA);
        if seen.contains(&found.id) {
            let repeat = seen.len() - 1;
            light = repeat_light[repeat];
            chroma *= REPEAT_CHROMA[repeat];
        }
        let lift = super::Lift { light, chroma, target: ACCENT_TARGET };
        accents.push(lift.fit(chroma, found.lch.hue, base, dark));
        seen.push(found.id);
    }
    let accent_hue = to_oklch(accents[0]).hue;
    let on_accent = if contrast(accents[0], BLACK) > contrast(accents[0], WHITE) {
        ON_DARK.at(accent_hue)
    } else {
        ON_LIGHT.at(accent_hue)
    };
    let draft = Draft {
        neutrals: [base, panel, raised, text, text.lerp(base, QUIET_MIX)],
        accent: accents[0],
        on_accent,
        second: accents[1],
        third: accents[2],
    };
    Made {
        palette: finish(&draft, analysis, dark, &TERMINAL),
        mark: Some([picked[0].x, picked[0].y]),
        shape: Shape::Plain,
    }
}
