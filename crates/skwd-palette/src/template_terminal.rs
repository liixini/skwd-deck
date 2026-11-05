use crate::color::{Rgb, from_hsl, to_hsl};

pub const TEMPLATE_HUES: [f32; 6] = [10.0, 145.0, 65.0, 230.0, 315.0, 185.0];
const SATURATION_FLOOR: f32 = 0.35;
const DARK_LIGHT: (f32, f32) = (0.6, 0.78);
const LIGHT_LIGHT: (f32, f32) = (0.3, 0.45);
const BRIGHT_STEP: f32 = 0.08;
const DARK_BRIGHT_CEILING: f32 = 0.9;
const LIGHT_BRIGHT_FLOOR: f32 = 0.2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TemplateTerminal {
    pub normal: [Rgb; TEMPLATE_HUES.len()],
    pub bright: [Rgb; TEMPLATE_HUES.len()],
}

pub fn template_terminal(primary: Rgb, dark: bool) -> TemplateTerminal {
    let (_, saturation, light) = to_hsl(primary);
    let saturation = saturation.max(SATURATION_FLOOR);
    let base = if dark {
        light.clamp(DARK_LIGHT.0, DARK_LIGHT.1)
    } else {
        light.clamp(LIGHT_LIGHT.0, LIGHT_LIGHT.1)
    };
    let lift = if dark {
        (base + BRIGHT_STEP).min(DARK_BRIGHT_CEILING)
    } else {
        (base - BRIGHT_STEP).max(LIGHT_BRIGHT_FLOOR)
    };
    TemplateTerminal {
        normal: TEMPLATE_HUES.map(|hue| from_hsl(hue, saturation, base)),
        bright: TEMPLATE_HUES.map(|hue| from_hsl(hue, saturation, lift)),
    }
}

#[cfg(test)]
#[path = "template_terminal_tests.rs"]
mod tests;
