use crate::{Rgb, ThemePalette};

use super::oklch::{Oklch, from_oklch, to_oklch};
use super::palette::{ANSI_SLOTS, StudioPalette};
#[cfg(feature = "material")]
use super::palette::{Role, mode_key};
use super::polish::{Pull, THEME_TERMINAL, polish, terminal};

const SECOND_CHROMA: f32 = 0.45;
const SECOND_DARK_CEILING: f32 = 0.8;
const SECOND_LIGHT_FLOOR: f32 = 0.45;
const CHROMATIC: f32 = 0.035;
const SHARES: [f32; 3] = [0.5, 0.3, 0.2];

fn softened(accent: Rgb, dark: bool) -> Rgb {
    let color = to_oklch(accent);
    let light = if dark {
        color.light.min(SECOND_DARK_CEILING)
    } else {
        color.light.max(SECOND_LIGHT_FLOOR)
    };
    from_oklch(Oklch { light, chroma: color.chroma * SECOND_CHROMA, hue: color.hue })
}

fn finished(mut palette: StudioPalette, dark: bool) -> StudioPalette {
    polish(&mut palette);
    let pulls: Vec<Pull> = [palette.accent, palette.second, palette.third]
        .into_iter()
        .zip(SHARES)
        .map(|(color, share)| (to_oklch(color), share))
        .filter(|(color, _)| color.chroma > CHROMATIC)
        .map(|(color, share)| Pull { hue: color.hue, strength: share * color.chroma })
        .collect();
    palette.ansi = terminal(&palette, &pulls, &THEME_TERMINAL, pulls.is_empty(), dark);
    palette
}

pub fn from_theme(theme: &ThemePalette) -> StudioPalette {
    let draft = StudioPalette {
        base: theme.background,
        panel: theme.surface,
        raised: theme.surface_variant,
        text: theme.on_surface,
        quiet: theme.outline,
        accent: theme.primary,
        on_accent: theme.on_primary,
        second: theme.primary,
        third: theme.tertiary,
        ansi: [theme.surface_variant; ANSI_SLOTS],
    };
    let dark = draft.is_dark();
    finished(StudioPalette { second: softened(theme.primary, dark), ..draft }, dark)
}

#[cfg(feature = "material")]
const MATERIAL_ROLES: [(Role, &str); 9] = [
    (Role::Base, "surface"),
    (Role::Panel, "surface_container"),
    (Role::Raised, "surface_container_high"),
    (Role::Text, "on_surface"),
    (Role::Quiet, "outline"),
    (Role::Accent, "primary"),
    (Role::OnAccent, "on_primary"),
    (Role::Second, "secondary"),
    (Role::Third, "tertiary"),
];

#[cfg(feature = "material")]
pub fn from_material(document: &serde_json::Value, dark: bool) -> Option<StudioPalette> {
    let mode = mode_key(dark);
    let black = Rgb(0, 0, 0);
    let mut palette = StudioPalette {
        base: black,
        panel: black,
        raised: black,
        text: black,
        quiet: black,
        accent: black,
        on_accent: black,
        second: black,
        third: black,
        ansi: [black; ANSI_SLOTS],
    };
    for (role, name) in MATERIAL_ROLES {
        let color =
            crate::material::role(document, name, mode).and_then(|hex| crate::parse_hex(&hex))?;
        palette.set(role, color);
    }
    palette.ansi = template_ansi(&palette, dark);
    Some(palette)
}

pub fn template_ansi(palette: &StudioPalette, dark: bool) -> [Rgb; ANSI_SLOTS] {
    let terminal = crate::template_terminal(palette.accent, dark);
    let half = ANSI_SLOTS / 2;
    let mut slots = [palette.base; ANSI_SLOTS];
    for (index, (normal, bright)) in terminal.normal.iter().zip(terminal.bright).enumerate() {
        slots[1 + index] = *normal;
        slots[half + 1 + index] = bright;
    }
    slots[half - 1] = palette.text;
    slots[half] = palette.quiet;
    slots[ANSI_SLOTS - 1] = palette.text;
    slots
}

#[cfg(test)]
#[path = "sources_tests.rs"]
mod tests;
