mod atmosphere;
mod gamut;
mod harmonic;
mod inkpaper;
mod mood;
mod pigment;
mod placement;
mod splittone;
mod subject;
mod temperature;
mod templates;
mod tonal;

use crate::Rgb;

use super::analysis::{Analysis, Cluster};
use super::oklch::{Oklch, oklch};
use super::palette::{ANSI_SLOTS, StudioPalette};
use super::polish::{Pull, Terminal, fit, polish, terminal};

const WHEEL_BINS: usize = 36;
const DEGREES_PER_BIN: f32 = 10.0;
const WARM_CENTRE: f32 = 50.0;
const WEIGHT_FLOOR: f32 = 1e-9;
const THEORY_GREY: f32 = 0.004;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Image,
    Theory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Tonal,
    Pigment,
    Subject,
    Placement,
    Atmosphere,
    Harmonic,
    Templates,
    Gamut,
    Temperature,
    Splittone,
    Mood,
    Inkpaper,
}

impl Method {
    pub const ALL: [Self; 12] = [
        Self::Tonal,
        Self::Pigment,
        Self::Subject,
        Self::Placement,
        Self::Atmosphere,
        Self::Harmonic,
        Self::Templates,
        Self::Gamut,
        Self::Temperature,
        Self::Splittone,
        Self::Mood,
        Self::Inkpaper,
    ];

    pub const fn key(self) -> &'static str {
        match self {
            Self::Tonal => "tonal",
            Self::Pigment => "pigment",
            Self::Subject => "subject",
            Self::Placement => "spatial",
            Self::Atmosphere => "atmosphere",
            Self::Harmonic => "harmonic",
            Self::Templates => "templates",
            Self::Gamut => "gamut",
            Self::Temperature => "temperature",
            Self::Splittone => "splittone",
            Self::Mood => "mood",
            Self::Inkpaper => "inkpaper",
        }
    }

    pub const fn family(self) -> Family {
        match self {
            Self::Tonal | Self::Pigment | Self::Subject | Self::Placement | Self::Atmosphere => {
                Family::Image
            }
            Self::Harmonic
            | Self::Templates
            | Self::Gamut
            | Self::Temperature
            | Self::Splittone
            | Self::Mood
            | Self::Inkpaper => Family::Theory,
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|method| method.key() == key)
    }

    pub fn make(self, analysis: &Analysis, dark: bool) -> Made {
        match self {
            Self::Tonal => tonal::make(analysis, dark),
            Self::Pigment => pigment::make(analysis, dark),
            Self::Subject => subject::make(analysis, dark),
            Self::Placement => placement::make(analysis, dark),
            Self::Atmosphere => atmosphere::make(analysis, dark),
            Self::Harmonic => harmonic::make(analysis, dark),
            Self::Templates => templates::make(analysis, dark),
            Self::Gamut => gamut::make(analysis, dark),
            Self::Temperature => temperature::make(analysis, dark),
            Self::Splittone => splittone::make(analysis, dark),
            Self::Mood => mood::make(analysis, dark),
            Self::Inkpaper => inkpaper::make(analysis, dark),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum Shape {
    #[default]
    Plain,
    Mask(Vec<[f32; 2]>),
    Sectors(Vec<[f32; 2]>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Made {
    pub palette: StudioPalette,
    pub mark: Option<[f32; 2]>,
    pub shape: Shape,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Tone {
    pub light: f32,
    pub chroma: f32,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Lift {
    pub light: f32,
    pub chroma: f32,
    pub target: f32,
}

pub(super) struct Neutrals {
    pub base: Tone,
    pub panel: Tone,
    pub raised: Tone,
    pub text: Tone,
    pub quiet: Tone,
}

pub(super) struct Hues {
    pub neutral: f32,
    pub chroma_scale: f32,
}

impl Tone {
    pub fn at(self, hue: f32) -> Rgb {
        oklch(self.light, self.chroma, hue)
    }
}

impl Lift {
    pub fn fit(self, chroma: f32, hue: f32, against: Rgb, dark: bool) -> Rgb {
        fit(Oklch { light: self.light, chroma, hue }, against, self.target, dark)
    }
}

pub(super) fn neutrals(spec: &Neutrals, hues: &Hues) -> [Rgb; 5] {
    let tone = |tone: Tone| oklch(tone.light, tone.chroma * hues.chroma_scale, hues.neutral);
    [tone(spec.base), tone(spec.panel), tone(spec.raised), tone(spec.text), tone(spec.quiet)]
}

pub(super) fn pulls(analysis: &Analysis) -> Vec<Pull> {
    analysis
        .chromatic()
        .map(|found| Pull { hue: found.lch.hue, strength: found.share * found.lch.chroma })
        .collect()
}

pub(super) struct Draft {
    pub neutrals: [Rgb; 5],
    pub accent: Rgb,
    pub on_accent: Rgb,
    pub second: Rgb,
    pub third: Rgb,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Pair {
    pub dark: f32,
    pub light: f32,
}

impl Pair {
    pub fn pick(self, dark: bool) -> f32 {
        if dark { self.dark } else { self.light }
    }
}

pub(super) fn strength(found: &Cluster) -> f32 {
    found.share * found.lch.chroma
}

pub(super) fn warmth(hue: f32) -> f32 {
    (hue - WARM_CENTRE).to_radians().cos()
}

pub(super) fn coloured(analysis: &Analysis) -> Vec<Cluster> {
    let found: Vec<Cluster> = analysis.chromatic().copied().collect();
    if found.is_empty() { vec![analysis.seed()] } else { found }
}

pub(super) fn mean_warmth(found: &[Cluster]) -> f32 {
    let (sum, total) = found.iter().fold((0.0f32, 0.0f32), |(sum, total), cluster| {
        let weight = strength(cluster) + WEIGHT_FLOOR;
        (sum + weight * warmth(cluster.lch.hue), total + weight)
    });
    sum / total
}

struct TheoryNeutrals {
    base: Pair,
    panel: Pair,
    raised: Pair,
    text: Pair,
    quiet: Pair,
    light_scale: f32,
    text_chroma: Pair,
}

const THEORY: TheoryNeutrals = TheoryNeutrals {
    base: Pair { dark: 0.17, light: 0.97 },
    panel: Pair { dark: 0.21, light: 0.94 },
    raised: Pair { dark: 0.26, light: 0.9 },
    text: Pair { dark: 0.93, light: 0.22 },
    quiet: Pair { dark: 0.66, light: 0.5 },
    light_scale: 0.7,
    text_chroma: Pair { dark: 0.015, light: 0.02 },
};

pub(super) fn theory_neutrals(hue: f32, chroma: f32, dark: bool) -> [Rgb; 5] {
    let surface = if dark { chroma } else { chroma * THEORY.light_scale };
    let tone = |light: Pair, chroma: f32| oklch(light.pick(dark), chroma, hue);
    [
        tone(THEORY.base, surface),
        tone(THEORY.panel, surface),
        tone(THEORY.raised, surface),
        tone(THEORY.text, chroma.min(THEORY.text_chroma.pick(dark))),
        tone(THEORY.quiet, chroma),
    ]
}

pub(super) fn grey_theory(analysis: &Analysis, dark: bool) -> Made {
    let neutrals = theory_neutrals(0.0, THEORY_GREY, dark);
    let [accent, second, third] = grey_accents(dark);
    let draft = Draft { neutrals, accent, on_accent: neutrals[0], second, third };
    Made {
        palette: finish(&draft, analysis, dark, &super::polish::WALLPAPER_TERMINAL),
        mark: None,
        shape: Shape::Plain,
    }
}

pub(super) fn grey_accents(dark: bool) -> [Rgb; 3] {
    const LIGHTS: [Pair; 3] = [
        Pair { dark: 0.78, light: 0.48 },
        Pair { dark: 0.72, light: 0.44 },
        Pair { dark: 0.84, light: 0.4 },
    ];
    const CHROMAS: [f32; 3] = [0.012, 0.01, 0.008];
    [0, 1, 2].map(|slot| oklch(LIGHTS[slot].pick(dark), CHROMAS[slot], 0.0))
}

pub(super) fn finish(
    draft: &Draft,
    analysis: &Analysis,
    dark: bool,
    spec: &Terminal,
) -> StudioPalette {
    let [base, panel, raised, text, quiet] = draft.neutrals;
    let mut palette = StudioPalette {
        base,
        panel,
        raised,
        text,
        quiet,
        accent: draft.accent,
        on_accent: draft.on_accent,
        second: draft.second,
        third: draft.third,
        ansi: [raised; ANSI_SLOTS],
    };
    polish(&mut palette);
    palette.ansi = terminal(&palette, &pulls(analysis), spec, analysis.grey, dark);
    palette
}

pub fn wheel(analysis: &Analysis) -> [f32; WHEEL_BINS] {
    let mut bins = [0.0f32; WHEEL_BINS];
    for found in analysis.chromatic() {
        let bin = (found.lch.hue / DEGREES_PER_BIN) as usize % WHEEL_BINS;
        bins[bin] += found.share * found.lch.chroma;
    }
    let peak = bins.iter().copied().fold(0.0, f32::max);
    if peak > 0.0 {
        for bin in &mut bins {
            *bin /= peak;
        }
    }
    bins
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
