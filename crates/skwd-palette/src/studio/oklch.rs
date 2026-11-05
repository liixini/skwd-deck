use crate::Rgb;
use crate::seed::linearize;

const TO_LMS: [[f32; 3]; 3] = [
    [0.412_221_47, 0.536_332_55, 0.051_445_995],
    [0.211_903_5, 0.680_699_5, 0.107_396_96],
    [0.088_302_46, 0.281_718_85, 0.629_978_7],
];
const TO_LAB: [[f32; 3]; 3] = [
    [0.210_454_26, 0.793_617_8, -0.004_072_047],
    [1.977_998_5, -2.428_592_2, 0.450_593_7],
    [0.025_904_037, 0.782_771_77, -0.808_675_77],
];
const FROM_LAB: [[f32; 3]; 3] = [
    [1.0, 0.396_337_78, 0.215_803_76],
    [1.0, -0.105_561_346, -0.063_854_17],
    [1.0, -0.089_484_18, -1.291_485_5],
];
const FROM_LMS: [[f32; 3]; 3] = [
    [4.076_741_7, -3.307_711_6, 0.230_969_94],
    [-1.268_438, 2.609_757_4, -0.341_319_4],
    [-0.004_196_086_3, -0.703_418_6, 1.707_614_7],
];
const GAMUT_SLACK: f32 = 1e-4;
const GAMUT_STEPS: usize = 20;
const LINEAR_KNEE: f32 = 0.003_130_8;
const LINEAR_SLOPE: f32 = 12.92;
const GAMMA: f32 = 2.4;
const GAMMA_SCALE: f32 = 1.055;
const GAMMA_OFFSET: f32 = 0.055;
pub const FULL_TURN: f32 = 360.0;
const HALF_TURN: f32 = 180.0;
const CUBE: i32 = 3;
pub const LAB_AXES: usize = 3;
pub type Lab = [f32; LAB_AXES];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Oklch {
    pub light: f32,
    pub chroma: f32,
    pub hue: f32,
}

fn apply(matrix: &[[f32; 3]; 3], vector: [f32; 3]) -> [f32; 3] {
    matrix.map(|row| row[0] * vector[0] + row[1] * vector[1] + row[2] * vector[2])
}

pub fn to_oklab(color: Rgb) -> Lab {
    let lms = apply(&TO_LMS, [linearize(color.0), linearize(color.1), linearize(color.2)]);
    apply(&TO_LAB, lms.map(f32::cbrt))
}

pub fn lab_to_lch(lab: Lab) -> Oklch {
    Oklch {
        light: lab[0],
        chroma: lab[1].hypot(lab[2]),
        hue: lab[2].atan2(lab[1]).to_degrees().rem_euclid(FULL_TURN),
    }
}

pub fn to_oklch(color: Rgb) -> Oklch {
    lab_to_lch(to_oklab(color))
}

fn linear_rgb(light: f32, chroma: f32, hue: f32) -> [f32; 3] {
    let radians = hue.to_radians();
    let lms = apply(&FROM_LAB, [light, chroma * radians.cos(), chroma * radians.sin()]);
    apply(&FROM_LMS, lms.map(|value| value.powi(CUBE)))
}

fn in_gamut(linear: [f32; 3]) -> bool {
    linear.iter().all(|value| (-GAMUT_SLACK..=1.0 + GAMUT_SLACK).contains(value))
}

fn encode(linear: f32) -> u8 {
    let value = linear.clamp(0.0, 1.0);
    let srgb = if value <= LINEAR_KNEE {
        value * LINEAR_SLOPE
    } else {
        GAMMA_SCALE * value.powf(GAMMA.recip()) - GAMMA_OFFSET
    };
    (srgb * f32::from(u8::MAX)).round() as u8
}

pub fn from_oklch(color: Oklch) -> Rgb {
    let light = color.light;
    let mut linear = linear_rgb(light, color.chroma.max(0.0), color.hue);
    if !in_gamut(linear) {
        let (mut low, mut high) = (0.0, color.chroma.max(0.0));
        for _ in 0..GAMUT_STEPS {
            let mid = f32::midpoint(low, high);
            if in_gamut(linear_rgb(light, mid, color.hue)) {
                low = mid;
            } else {
                high = mid;
            }
        }
        linear = linear_rgb(light, low, color.hue);
    }
    Rgb(encode(linear[0]), encode(linear[1]), encode(linear[2]))
}

pub fn oklch(light: f32, chroma: f32, hue: f32) -> Rgb {
    from_oklch(Oklch { light, chroma, hue })
}

pub fn hue_distance(from: f32, to: f32) -> f32 {
    let difference = (from - to).abs() % FULL_TURN;
    difference.min(FULL_TURN - difference)
}

pub fn hue_offset(from: f32, to: f32) -> f32 {
    (to - from + FULL_TURN + HALF_TURN).rem_euclid(FULL_TURN) - HALF_TURN
}

pub fn mix_hue(from: f32, to: f32, amount: f32) -> f32 {
    (from + hue_offset(from, to) * amount).rem_euclid(FULL_TURN)
}

#[cfg(test)]
#[path = "oklch_tests.rs"]
mod tests;
