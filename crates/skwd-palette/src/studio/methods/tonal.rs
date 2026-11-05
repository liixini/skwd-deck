use super::super::analysis::Analysis;
use super::super::oklch::FULL_TURN;
use super::super::polish::WALLPAPER_TERMINAL;
use super::{Draft, Hues, Lift, Made, Neutrals, Shape, Tone, finish, neutrals};

struct Spec {
    neutrals: Neutrals,
    accent: Lift,
    second: Lift,
    third: Lift,
    on_accent: Tone,
}

const GREY_CHROMA: f32 = 0.02;
const ACCENT_CHROMA: (f32, f32) = (0.06, 0.14);
const SECOND_STEP: f32 = 30.0;
const THIRD_STEP: f32 = 60.0;
const OTHERS: usize = 2;
const OTHER_GAP: f32 = 40.0;

const DARK: Spec = Spec {
    neutrals: Neutrals {
        base: Tone { light: 0.17, chroma: 0.012 },
        panel: Tone { light: 0.21, chroma: 0.014 },
        raised: Tone { light: 0.26, chroma: 0.016 },
        text: Tone { light: 0.93, chroma: 0.012 },
        quiet: Tone { light: 0.66, chroma: 0.02 },
    },
    accent: Lift { light: 0.80, chroma: 0.0, target: 7.0 },
    second: Lift { light: 0.72, chroma: 0.07, target: 6.0 },
    third: Lift { light: 0.86, chroma: 0.09, target: 6.0 },
    on_accent: Tone { light: 0.22, chroma: 0.05 },
};

const LIGHT: Spec = Spec {
    neutrals: Neutrals {
        base: Tone { light: 0.975, chroma: 0.008 },
        panel: Tone { light: 0.945, chroma: 0.012 },
        raised: Tone { light: 0.905, chroma: 0.016 },
        text: Tone { light: 0.22, chroma: 0.015 },
        quiet: Tone { light: 0.50, chroma: 0.02 },
    },
    accent: Lift { light: 0.48, chroma: 0.0, target: 5.5 },
    second: Lift { light: 0.50, chroma: 0.08, target: 4.5 },
    third: Lift { light: 0.40, chroma: 0.10, target: 4.5 },
    on_accent: Tone { light: 0.98, chroma: 0.01 },
};

pub(super) fn make(analysis: &Analysis, dark: bool) -> Made {
    let spec = if dark { &DARK } else { &LIGHT };
    let seed = analysis.seed();
    let hue = seed.lch.hue;
    let others = analysis.others(hue, OTHERS, OTHER_GAP);
    let (second_hue, third_hue) = if analysis.grey {
        (hue, hue)
    } else {
        (
            others.first().map_or((hue + SECOND_STEP).rem_euclid(FULL_TURN), |found| found.lch.hue),
            others.get(1).map_or((hue + THIRD_STEP).rem_euclid(FULL_TURN), |found| found.lch.hue),
        )
    };
    let accent_chroma = if analysis.grey {
        GREY_CHROMA
    } else {
        seed.lch.chroma.clamp(ACCENT_CHROMA.0, ACCENT_CHROMA.1)
    };
    let chroma = |lift: Lift| if analysis.grey { GREY_CHROMA } else { lift.chroma };
    let neutrals = neutrals(&spec.neutrals, &Hues { neutral: hue, chroma_scale: 1.0 });
    let base = neutrals[0];
    let draft = Draft {
        neutrals,
        accent: spec.accent.fit(accent_chroma, hue, base, dark),
        on_accent: spec.on_accent.at(hue),
        second: spec.second.fit(chroma(spec.second), second_hue, base, dark),
        third: spec.third.fit(chroma(spec.third), third_hue, base, dark),
    };
    Made {
        palette: finish(&draft, analysis, dark, &WALLPAPER_TERMINAL),
        mark: Some([seed.x, seed.y]),
        shape: Shape::Plain,
    }
}
