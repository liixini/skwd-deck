use crate::seed::{Swatch, chroma_of, swatches};
use crate::{Rgb, from_hsl, to_hsl};

pub const CLUSTERS: usize = 14;
const NUDGE_STEPS: usize = 130;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Semantic {
    pub bg: Rgb,
    pub surface: Rgb,
    pub fg: Rgb,
    pub dim: Rgb,
    pub accent: Rgb,
}

#[derive(Clone, Copy)]
struct Flags {
    grey: bool,
    pure_black: bool,
    pure_white: bool,
}

#[derive(Clone, Copy)]
struct Entry {
    hue: f32,
    sat: f32,
    light: f32,
    chroma: f32,
    mass: f32,
    significance: f32,
}

fn entry_of(swatch: &Swatch) -> Entry {
    let significance =
        swatch.chroma * 0.50 + swatch.sat * 100.0 * 0.30 + swatch.share * 100.0 * 0.20;
    Entry {
        hue: swatch.hue,
        sat: swatch.sat,
        light: swatch.light,
        chroma: swatch.chroma,
        mass: swatch.share,
        significance,
    }
}

fn relative_luminance(color: Rgb) -> f32 {
    let linearize = |channel: u8| {
        let value = f32::from(channel) / 255.0;
        if value <= 0.040_45 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * linearize(color.0) + 0.7152 * linearize(color.1) + 0.0722 * linearize(color.2)
}

pub fn contrast(a: Rgb, b: Rgb) -> f32 {
    let (a_luminance, b_luminance) = (relative_luminance(a), relative_luminance(b));
    let (high, low) = if a_luminance > b_luminance {
        (a_luminance, b_luminance)
    } else {
        (b_luminance, a_luminance)
    };
    (high + 0.05) / (low + 0.05)
}

fn hue_distance(a: f32, b: f32) -> f32 {
    let difference = (a - b).abs() % 360.0;
    if difference > 180.0 { 360.0 - difference } else { difference }
}

fn nudge_light(
    hue: f32,
    saturation: f32,
    light: f32,
    target: f32,
    against: Rgb,
    darker: bool,
) -> f32 {
    let step = if darker { -0.005 } else { 0.005 };
    let limit = if darker { 0.18 } else { 0.82 };
    let mut light = light;
    for _ in 0..NUDGE_STEPS {
        if contrast(from_hsl(hue, saturation, light), against) >= target {
            return light;
        }
        light += step;
        if darker && light < limit {
            return limit;
        }
        if !darker && light > limit {
            return limit;
        }
    }
    light
}

fn lowest_light(entries: &[Entry]) -> Entry {
    entries.iter().copied().min_by(|a, b| a.light.total_cmp(&b.light)).unwrap_or(entries[0])
}

fn highest_light(entries: &[Entry]) -> Entry {
    entries.iter().copied().max_by(|a, b| a.light.total_cmp(&b.light)).unwrap_or(entries[0])
}

fn assign_background(entries: &[Entry], dark: bool, flags: Flags) -> Rgb {
    if flags.grey {
        return match (dark, flags.pure_black, flags.pure_white) {
            (true, true, _) => from_hsl(0.0, 0.005, 0.015),
            (true, false, _) => from_hsl(0.0, 0.02, 0.13),
            (false, _, true) => from_hsl(0.0, 0.005, 0.985),
            (false, _, false) => from_hsl(0.0, 0.02, 0.87),
        };
    }
    let mut by_mass = entries.to_vec();
    by_mass.sort_by(|a, b| b.mass.total_cmp(&a.mass));
    if dark {
        if flags.pure_black {
            return from_hsl(0.0, 0.005, 0.015);
        }
        let source = by_mass
            .iter()
            .find(|entry| entry.light < 0.52)
            .copied()
            .unwrap_or_else(|| lowest_light(entries));
        from_hsl(source.hue, source.sat, source.light.clamp(0.12, 0.28))
    } else {
        if flags.pure_white {
            return from_hsl(0.0, 0.005, 0.985);
        }
        let source = by_mass
            .iter()
            .find(|entry| entry.light > 0.48)
            .copied()
            .unwrap_or_else(|| highest_light(entries));
        from_hsl(source.hue, source.sat, source.light.clamp(0.72, 0.92))
    }
}

fn assign_foreground(entries: &[Entry], background: Rgb, dark: bool, grey: bool) -> Rgb {
    if grey {
        return if dark { from_hsl(0.0, 0.01, 0.88) } else { from_hsl(0.0, 0.01, 0.12) };
    }
    if dark {
        let source = highest_light(entries);
        let target = if source.light > 0.60 { source.light } else { 0.82 };
        let light = target.clamp(0.75, 0.91);
        let light = nudge_light(source.hue, source.sat, light, 4.5, background, false);
        from_hsl(source.hue, source.sat, light.clamp(0.72, 0.91))
    } else {
        let source = lowest_light(entries);
        let saturation = (source.sat * 0.60).min(0.25);
        let light = nudge_light(source.hue, saturation, 0.11, 7.0, background, true);
        from_hsl(source.hue, saturation, light.clamp(0.08, 0.20))
    }
}

fn assign_surface(background: Rgb, dark: bool) -> Rgb {
    let (hue, saturation, light) = to_hsl(background);
    let light =
        if dark { (light + 0.07).clamp(0.10, 0.36) } else { (light - 0.07).clamp(0.60, 0.90) };
    from_hsl(hue, saturation, light)
}

fn assign_dim(background: Rgb, foreground: Rgb, grey: bool) -> Rgb {
    let (hue, saturation, background_light) = to_hsl(background);
    let (_, _, foreground_light) = to_hsl(foreground);
    let middle = background_light + (foreground_light - background_light) * 0.38;
    if grey {
        return from_hsl(0.0, 0.01, middle);
    }
    from_hsl(hue, (saturation * 0.65).clamp(0.04, 0.40), middle)
}

fn grey_accent(dark: bool) -> Rgb {
    if dark { from_hsl(220.0, 0.22, 0.68) } else { from_hsl(220.0, 0.35, 0.38) }
}

fn assign_accent(entries: &[Entry], background: Rgb, dark: bool, grey: bool) -> Rgb {
    if grey {
        return grey_accent(dark);
    }
    let (background_hue, _, _) = to_hsl(background);
    let best = entries
        .iter()
        .copied()
        .max_by(|a, b| {
            let score = |entry: &Entry| {
                entry.significance * 0.60
                    + (hue_distance(entry.hue, background_hue) / 90.0).min(1.0) * 40.0 * 0.40
            };
            score(a).total_cmp(&score(b))
        })
        .unwrap_or(entries[0]);
    if dark {
        let mut light = best.light;
        if light < 0.45 {
            light = 0.58 + (best.chroma / 60.0) * 0.14;
        }
        light = light.clamp(0.48, 0.80);
        let light = nudge_light(best.hue, best.sat, light, 3.0, background, false);
        from_hsl(best.hue, best.sat, light.clamp(0.45, 0.82))
    } else {
        let mut light = best.light;
        if light > 0.55 {
            light = 0.36 - (best.chroma / 60.0) * 0.08;
        }
        light = light.clamp(0.22, 0.52);
        let light = nudge_light(best.hue, best.sat, light, 4.0, background, true);
        from_hsl(best.hue, best.sat, light.clamp(0.18, 0.55))
    }
}

pub fn semantic(rgba: &[u8], width: usize, height: usize, dark: bool) -> Option<Semantic> {
    let found = swatches(rgba, width, height, CLUSTERS);
    if found.is_empty() {
        return None;
    }
    let entries: Vec<Entry> = found.iter().map(entry_of).collect();
    let tone = crate::seed::tone(rgba);
    let grey = tone.greyscale();
    let background = assign_background(
        &entries,
        dark,
        Flags { grey, pure_black: tone.pure_black(), pure_white: tone.pure_white() },
    );
    let foreground = assign_foreground(&entries, background, dark, grey);
    let mut accent = assign_accent(&entries, background, dark, grey);
    if chroma_of(accent) < crate::seed::ACHROMATIC_CHROMA {
        accent = grey_accent(dark);
    }
    Some(Semantic {
        bg: background,
        surface: assign_surface(background, dark),
        fg: foreground,
        dim: assign_dim(background, foreground, grey),
        accent,
    })
}

pub fn accent_chroma(semantic: &Semantic) -> f32 {
    chroma_of(semantic.accent)
}

#[cfg(test)]
#[path = "scheme_tests.rs"]
mod tests;
