use super::sampling::sample;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tone {
    pub grey_ratio: f32,
    pub true_black: f32,
    pub true_white: f32,
    pub avg_light: f32,
    pub dark_ratio: f32,
    pub light_ratio: f32,
}

impl Tone {
    pub fn greyscale(&self) -> bool {
        self.grey_ratio > 0.70 && self.true_black < 0.30 && self.true_white < 0.30
    }

    pub fn pure_black(&self) -> bool {
        self.true_black > 0.45
    }

    pub fn pure_white(&self) -> bool {
        self.true_white > 0.45
    }

    pub fn prefers_dark(&self) -> bool {
        if self.dark_ratio > 0.52 {
            return true;
        }
        if self.light_ratio > 0.52 {
            return false;
        }
        self.avg_light < 0.50
    }
}

pub fn tone(rgba: &[u8]) -> Tone {
    let samples = sample(rgba, 0, 0);
    if samples.is_empty() {
        return Tone {
            grey_ratio: 0.0,
            true_black: 0.0,
            true_white: 0.0,
            avg_light: 0.0,
            dark_ratio: 0.0,
            light_ratio: 0.0,
        };
    }
    let (mut grey, mut black, mut white, mut dim, mut bright) = (0u32, 0u32, 0u32, 0u32, 0u32);
    let mut light_sum = 0.0f64;
    for (color, _) in &samples {
        let (_, saturation, light) = crate::to_hsl(*color);
        light_sum += f64::from(light);
        if saturation < 0.12 {
            grey += 1;
        }
        if light < 0.05 {
            black += 1;
        }
        if light > 0.95 {
            white += 1;
        }
        if light < 0.40 {
            dim += 1;
        }
        if light > 0.60 {
            bright += 1;
        }
    }
    let total = samples.len() as f32;
    Tone {
        grey_ratio: grey as f32 / total,
        true_black: black as f32 / total,
        true_white: white as f32 / total,
        avg_light: (light_sum / f64::from(total)) as f32,
        dark_ratio: dim as f32 / total,
        light_ratio: bright as f32 / total,
    }
}

#[cfg(test)]
#[path = "tone_tests.rs"]
mod tests;
