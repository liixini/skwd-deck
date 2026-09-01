#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }

    pub fn lum(self) -> f32 {
        0.2126 * self.0 as f32 + 0.7152 * self.1 as f32 + 0.0722 * self.2 as f32
    }

    #[must_use]
    pub fn lerp(self, to: Rgb, frac: f32) -> Rgb {
        let mix = |from: u8, to: u8| {
            (f32::from(from) + (f32::from(to) - f32::from(from)) * frac).round() as u8
        };
        Rgb(mix(self.0, to.0), mix(self.1, to.1), mix(self.2, to.2))
    }
}

pub fn parse_hex(text: &str) -> Option<Rgb> {
    let digits = text.trim().trim_start_matches('#');
    if digits.len() < 6 {
        return None;
    }
    let chan = |idx: usize| u8::from_str_radix(digits.get(idx..idx + 2)?, 16).ok();
    Some(Rgb(chan(0)?, chan(2)?, chan(4)?))
}

pub fn to_hsl(color: Rgb) -> (f32, f32, f32) {
    let red = color.0 as f32 / 255.0;
    let green = color.1 as f32 / 255.0;
    let blue = color.2 as f32 / 255.0;
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let light = f32::midpoint(max, min);
    let delta = max - min;
    if delta.abs() < 1e-6 {
        return (0.0, 0.0, light);
    }
    let saturation = delta / (1.0 - (2.0 * light - 1.0).abs());
    let hue = if max == red {
        60.0 * (((green - blue) / delta) % 6.0)
    } else if max == green {
        60.0 * (((blue - red) / delta) + 2.0)
    } else {
        60.0 * (((red - green) / delta) + 4.0)
    };
    ((hue + 360.0) % 360.0, saturation.clamp(0.0, 1.0), light)
}

pub fn from_hsl(hue: f32, saturation: f32, light: f32) -> Rgb {
    let hue = ((hue % 360.0) + 360.0) % 360.0;
    let saturation = saturation.clamp(0.0, 1.0);
    let light = light.clamp(0.0, 1.0);
    let chroma = (1.0 - (2.0 * light - 1.0).abs()) * saturation;
    let secondary = chroma * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let offset = light - chroma / 2.0;
    let (red, green, blue) = match (hue / 60.0) as u32 {
        0 => (chroma, secondary, 0.0),
        1 => (secondary, chroma, 0.0),
        2 => (0.0, chroma, secondary),
        3 => (0.0, secondary, chroma),
        4 => (secondary, 0.0, chroma),
        _ => (chroma, 0.0, secondary),
    };
    let channel = |value: f32| ((value + offset) * 255.0).round().clamp(0.0, 255.0) as u8;
    Rgb(channel(red), channel(green), channel(blue))
}

pub fn rotate(color: Rgb, degrees: f32) -> Rgb {
    let (hue, saturation, light) = to_hsl(color);
    from_hsl((hue + degrees).rem_euclid(360.0), saturation, light)
}

#[cfg(test)]
#[path = "color_tests.rs"]
mod tests;
