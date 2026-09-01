use crate::Rgb;

pub const CENTRE_BIAS: f32 = 0.18;
const MAX_SAMPLES: usize = 20_000;

fn centre_weight(index: usize, width: usize, height: usize) -> f32 {
    if width < 2 || height < 2 {
        return 1.0;
    }
    let x = index % width;
    let y = index / width;
    if y >= height {
        return 1.0;
    }
    let center_x = 2.0 * x as f32 / (width - 1) as f32 - 1.0;
    let center_y = 2.0 * y as f32 / (height - 1) as f32 - 1.0;
    1.0 - (center_x.hypot(center_y) / std::f32::consts::SQRT_2) * CENTRE_BIAS
}

pub(super) fn sample(rgba: &[u8], width: usize, height: usize) -> Vec<(Rgb, f32)> {
    let pixel_count = rgba.len() / 4;
    let sized = width >= 2 && height >= 2 && width * height >= pixel_count;
    let step = (pixel_count / MAX_SAMPLES).max(1);
    let mut samples = Vec::with_capacity(MAX_SAMPLES.min(pixel_count));
    let mut index = 0;
    while index < pixel_count {
        let offset = index * 4;
        if rgba[offset + 3] >= 128 {
            let weight = if sized { centre_weight(index, width, height) } else { 1.0 };
            samples.push((Rgb(rgba[offset], rgba[offset + 1], rgba[offset + 2]), weight));
        }
        index += step;
    }
    samples
}
