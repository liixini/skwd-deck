use crate::{Rgb, from_hsl, quantize, to_hsl};

#[derive(Clone, Copy, Debug)]
pub struct ThemePalette {
    pub primary: Rgb,
    pub on_primary: Rgb,
    pub surface: Rgb,
    pub on_surface: Rgb,
    pub surface_variant: Rgb,
    pub surface_container: Rgb,
    pub background: Rgb,
    pub outline: Rgb,
    pub tertiary: Rgb,
}

fn pick_accent(dominant: &[Rgb]) -> (f32, f32) {
    dominant
        .iter()
        .map(|color| {
            let (hue, saturation, light) = to_hsl(*color);
            (hue, saturation, light)
        })
        .max_by(|a, b| {
            let score = |hsl: &(f32, f32, f32)| hsl.1 * (1.0 - (hsl.2 - 0.55).abs());
            score(a).total_cmp(&score(b))
        })
        .map_or((265.0, 0.45), |(hue, saturation, _)| (hue, saturation.max(0.35)))
}

pub fn derive(dominant: &[Rgb], dark: bool) -> ThemePalette {
    let (hue, saturation) = pick_accent(dominant);
    let tertiary_hue = (hue + 60.0) % 360.0;
    let tint = (saturation * 0.4).clamp(0.08, 0.22);
    if dark {
        ThemePalette {
            primary: from_hsl(hue, (saturation * 0.9).clamp(0.4, 0.85), 0.78),
            on_primary: from_hsl(hue, 0.5, 0.12),
            background: from_hsl(hue, tint, 0.07),
            surface: from_hsl(hue, tint, 0.10),
            surface_container: from_hsl(hue, tint, 0.14),
            surface_variant: from_hsl(hue, tint * 0.9, 0.28),
            on_surface: from_hsl(hue, 0.10, 0.92),
            outline: from_hsl(hue, 0.12, 0.55),
            tertiary: from_hsl(tertiary_hue, (saturation * 0.8).clamp(0.35, 0.7), 0.75),
        }
    } else {
        ThemePalette {
            primary: from_hsl(hue, (saturation * 0.9).clamp(0.45, 0.9), 0.38),
            on_primary: from_hsl(hue, 0.25, 0.97),
            background: from_hsl(hue, tint * 0.6, 0.96),
            surface: from_hsl(hue, tint * 0.6, 0.93),
            surface_container: from_hsl(hue, tint * 0.6, 0.88),
            surface_variant: from_hsl(hue, tint * 0.7, 0.78),
            on_surface: from_hsl(hue, 0.30, 0.12),
            outline: from_hsl(hue, 0.18, 0.45),
            tertiary: from_hsl(tertiary_hue, (saturation * 0.8).clamp(0.4, 0.75), 0.4),
        }
    }
}

pub fn generate(rgba: &[u8], dark: bool) -> ThemePalette {
    derive(&quantize(rgba, 8), dark)
}

#[cfg(test)]
#[path = "theme_tests.rs"]
mod tests;
