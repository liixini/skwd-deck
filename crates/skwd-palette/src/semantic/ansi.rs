use crate::{Rgb, from_hsl, to_hsl};

pub const ANSI_SNAP_DEGREES: f32 = 35.0;
pub const ANSI_HUES: [f32; 6] = [0.0, 120.0, 55.0, 220.0, 300.0, 190.0];
pub const ANSI_VARIANTS: [&str; 4] = ["dark16", "harddark", "softdark", "light16"];

fn darken(color: Rgb, amount: f32) -> Rgb {
    let (hue, saturation, light) = to_hsl(color);
    from_hsl(hue, saturation, (light * (1.0 - amount)).clamp(0.0, 1.0))
}

fn blend(color: Rgb, toward: Rgb, fraction: f32) -> Rgb {
    let mix = |a: u8, b: u8| {
        (f32::from(a) + (f32::from(b) - f32::from(a)) * fraction).round().clamp(0.0, 255.0) as u8
    };
    Rgb(mix(color.0, toward.0), mix(color.1, toward.1), mix(color.2, toward.2))
}

pub fn ansi16(rgba: &[u8], width: usize, height: usize, dark: bool) -> Option<Vec<Rgb>> {
    let mut swatches = crate::seed::swatches(rgba, width, height, 16);
    if swatches.is_empty() {
        return None;
    }
    swatches.sort_by(|a, b| to_hsl(a.color).2.total_cmp(&to_hsl(b.color).2));
    let last = swatches.len() - 1;
    let pick = |slot: usize| swatches[(slot * last) / 7].color;
    let near_white = Rgb(0xee, 0xee, 0xee);

    let mut colors: Vec<Rgb> = (0..8).map(pick).collect();
    if dark {
        colors[0] = darken(pick(0), 0.40);
        colors[7] = blend(pick(7), near_white, 0.35);
    } else {
        colors[0] = blend(pick(7), near_white, 0.55);
        colors[7] = darken(pick(0), 0.55);
    }
    let bright: Vec<Rgb> = (1..7).map(|index| blend(colors[index], near_white, 0.18)).collect();
    let mut output = colors.clone();
    output.push(if dark { darken(colors[7], 0.30) } else { blend(colors[7], near_white, 0.30) });
    output.extend(bright);
    output.push(blend(colors[7], near_white, 0.20));
    Some(output)
}

fn shift(color: Rgb, delta: f32) -> Rgb {
    let (hue, saturation, light) = to_hsl(color);
    from_hsl(hue, saturation, (light + delta).clamp(0.0, 1.0))
}

fn saturate(color: Rgb, factor: f32) -> Rgb {
    let (hue, saturation, light) = to_hsl(color);
    from_hsl(hue, (saturation * factor).clamp(0.0, 1.0), light)
}

pub fn ansi16_variant(
    rgba: &[u8],
    width: usize,
    height: usize,
    dark: bool,
    variant: &str,
) -> Option<Vec<Rgb>> {
    let light_mode = variant == "light16" || (!dark && variant != "dark16");
    let mut colors = ansi16(rgba, width, height, !light_mode)?;
    let (background_delta, foreground_delta, saturation): (f32, f32, f32) = match variant {
        "harddark" => (-0.08, 0.10, 1.20),
        "softdark" => (0.06, -0.06, 0.80),
        _ => (0.0, 0.0, 1.0),
    };
    if background_delta != 0.0 || foreground_delta != 0.0 || (saturation - 1.0).abs() > f32::EPSILON
    {
        colors[0] = shift(colors[0], background_delta);
        colors[8] = shift(colors[8], background_delta);
        for slot in [1, 2, 3, 4, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15] {
            colors[slot] = saturate(shift(colors[slot], foreground_delta), saturation);
        }
    }
    Some(colors)
}

#[cfg(test)]
#[path = "ansi_tests.rs"]
mod tests;
