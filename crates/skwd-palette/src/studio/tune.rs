use super::oklch::{Oklch, from_oklch, mix_hue, to_oklch};
use super::palette::{Role, StudioPalette, TERMINAL_KEYS};
use super::polish::{READABLE_CONTRAST, ensure};

pub const WARMTH_RANGE: (f32, f32) = (-1.0, 1.0);
pub const COLOUR_RANGE: (f32, f32) = (0.4, 1.6);
pub const CONTRAST_RANGE: (f32, f32) = (-1.0, 1.0);
pub const SHADE_RANGE: (i8, i8) = (-3, 3);
pub const SHADE_STEP: f32 = 0.045;
const WARM_HUE: f32 = 65.0;
const COOL_HUE: f32 = 250.0;
const WARM_HUE_SHARE: f32 = 0.75;
const WARM_CHROMA: f32 = 0.022;
const GREY_CHROMA: f32 = 0.01;
const CONTRAST_SPAN: f32 = 2.0;

struct Shift {
    role: Role,
    dark: f32,
    light: f32,
}

const CONTRAST_SHIFTS: [Shift; 5] = [
    Shift { role: Role::Base, dark: -0.025, light: 0.01 },
    Shift { role: Role::Panel, dark: -0.022, light: 0.01 },
    Shift { role: Role::Raised, dark: -0.02, light: 0.008 },
    Shift { role: Role::Text, dark: 0.015, light: -0.03 },
    Shift { role: Role::Quiet, dark: 0.01, light: -0.02 },
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tuning {
    pub warmth: f32,
    pub colour: f32,
    pub contrast: f32,
    pub shades: [i8; Role::ALL.len()],
}

impl Default for Tuning {
    fn default() -> Self {
        Self { warmth: 0.0, colour: 1.0, contrast: 0.0, shades: [0; Role::ALL.len()] }
    }
}

impl Tuning {
    pub fn from_parts(
        warmth: f32,
        colour: f32,
        contrast: f32,
        shades: impl IntoIterator<Item = i64>,
    ) -> Self {
        let mut tuning = Self { warmth, colour, contrast, ..Self::default() };
        for (slot, step) in tuning.shades.iter_mut().zip(shades) {
            let clamped = step.clamp(i64::from(SHADE_RANGE.0), i64::from(SHADE_RANGE.1));
            *slot = i8::try_from(clamped).unwrap_or_default();
        }
        tuning
    }

    pub fn shade(&self, role: Role) -> f32 {
        f32::from(self.shades[role.index()].clamp(SHADE_RANGE.0, SHADE_RANGE.1)) * SHADE_STEP
    }

    fn neutral(&self, color: Oklch, lift: f32) -> Oklch {
        let warmth = self.warmth.clamp(WARMTH_RANGE.0, WARMTH_RANGE.1);
        if warmth == 0.0 {
            return Oklch { light: color.light + lift, ..color };
        }
        let strength = warmth.abs();
        let target = if warmth > 0.0 { WARM_HUE } else { COOL_HUE };
        let hue = if color.chroma < GREY_CHROMA {
            target
        } else {
            mix_hue(color.hue, target, strength * WARM_HUE_SHARE)
        };
        Oklch { light: color.light + lift, chroma: color.chroma + WARM_CHROMA * strength, hue }
    }
}

pub fn tune(palette: &StudioPalette, tuning: &Tuning, dark: bool) -> StudioPalette {
    let mut out = *palette;
    let contrast = tuning.contrast.clamp(CONTRAST_RANGE.0, CONTRAST_RANGE.1) * CONTRAST_SPAN;
    let colour = tuning.colour.clamp(COLOUR_RANGE.0, COLOUR_RANGE.1);
    for shift in &CONTRAST_SHIFTS {
        let step = if dark { shift.dark } else { shift.light };
        let lift = step * contrast + tuning.shade(shift.role);
        out.set(shift.role, from_oklch(tuning.neutral(to_oklch(palette.get(shift.role)), lift)));
    }
    for role in Role::ACCENTS {
        let color = to_oklch(palette.get(role));
        out.set(
            role,
            from_oklch(Oklch {
                light: color.light + tuning.shade(role),
                chroma: color.chroma * colour,
                ..color
            }),
        );
    }
    for (_, slot) in TERMINAL_KEYS {
        let color = to_oklch(palette.ansi[slot]);
        out.ansi[slot] = from_oklch(Oklch { chroma: color.chroma * colour, ..color });
    }
    out.on_accent = ensure(out.on_accent, out.accent, READABLE_CONTRAST);
    out
}

#[cfg(test)]
#[path = "tune_tests.rs"]
mod tests;
