use crate::{Rgb, from_hsl, rotate, to_hsl};

use super::scheme::{Semantic, contrast};

pub const LAYER_VARIANT_CONTRAST: f32 = 1.28;
pub const LAYER_CONTAINER_CONTRAST: f32 = 1.62;
pub const LAYER_BACKDROP_CONTRAST: f32 = 1.18;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiPalette {
    pub primary: Rgb,
    pub primary_text: Rgb,
    pub tertiary: Rgb,
    pub surface: Rgb,
    pub surface_text: Rgb,
    pub surface_variant: Rgb,
    pub surface_container: Rgb,
    pub background: Rgb,
    pub outline: Rgb,
}

fn step_light(color: Rgb, delta: f32) -> Rgb {
    let (hue, saturation, light) = to_hsl(color);
    from_hsl(hue, saturation, (light + delta).clamp(0.03, 0.97))
}

fn layer_in_direction(base: Rgb, direction: f32, target: f32) -> Rgb {
    let mut delta = 0.02f32;
    let mut output = step_light(base, direction * delta);
    while delta < 0.30 && contrast(base, output) < target {
        delta += 0.01;
        output = step_light(base, direction * delta);
    }
    output
}

fn layer(base: Rgb, direction: f32, target: f32) -> Rgb {
    let wanted = layer_in_direction(base, direction, target);
    if contrast(base, wanted) >= target {
        return wanted;
    }
    let flipped = layer_in_direction(base, -direction, target);
    if contrast(base, flipped) > contrast(base, wanted) { flipped } else { wanted }
}

pub fn derive_ui_palette(semantic: &Semantic) -> UiPalette {
    let direction = if to_hsl(semantic.fg).2 > to_hsl(semantic.bg).2 { 1.0 } else { -1.0 };
    UiPalette {
        primary: semantic.accent,
        primary_text: semantic.bg,
        tertiary: rotate(semantic.accent, 60.0),
        surface: semantic.bg,
        surface_text: semantic.fg,
        surface_variant: layer(semantic.bg, direction, LAYER_VARIANT_CONTRAST),
        surface_container: layer(semantic.bg, direction, LAYER_CONTAINER_CONTRAST),
        background: layer(semantic.bg, -direction, LAYER_BACKDROP_CONTRAST),
        outline: semantic.dim,
    }
}

#[cfg(test)]
#[path = "layers_tests.rs"]
mod tests;
