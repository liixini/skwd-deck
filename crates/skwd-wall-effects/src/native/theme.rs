use image::{DynamicImage, RgbaImage};
use rayon::prelude::*;
use serde_json::Value;

use skwd_palette::gowall as themes;

pub(crate) fn apply(image: DynamicImage, params: &Value) -> anyhow::Result<DynamicImage> {
    let name = params.get("theme").and_then(Value::as_str).unwrap_or("Catppuccin");
    let palette = themes::lookup(name).ok_or_else(|| anyhow::anyhow!("unknown theme: {name}"))?;
    if palette.is_empty() {
        anyhow::bail!("theme {name} has no colours");
    }

    let lookup = build_palette_lut(palette, 50.0);
    let rgba = image.into_rgba8();
    let (width, height) = (rgba.width(), rgba.height());
    let mut raw = rgba.into_raw();

    raw.par_chunks_exact_mut(4).for_each(|chunk| {
        let red = (chunk[0] >> 3) as usize;
        let green = (chunk[1] >> 3) as usize;
        let blue = (chunk[2] >> 3) as usize;
        let entry = lookup[(red << 10) | (green << 5) | blue];
        chunk[0] = entry[0];
        chunk[1] = entry[1];
        chunk[2] = entry[2];
    });

    let output = RgbaImage::from_raw(width, height, raw)
        .ok_or_else(|| anyhow::anyhow!("failed to rebuild image buffer"))?;
    Ok(DynamicImage::ImageRgba8(output))
}

fn build_palette_lut(palette: &[(u8, u8, u8)], sigma: f32) -> Vec<[u8; 3]> {
    const GRID_SIZE: usize = 32;
    let two_sigma_squared = 2.0 * sigma * sigma;
    let palette: Vec<(f32, f32, f32)> = palette
        .iter()
        .map(|&(red, green, blue)| (f32::from(red), f32::from(green), f32::from(blue)))
        .collect();

    (0..GRID_SIZE * GRID_SIZE * GRID_SIZE)
        .into_par_iter()
        .map(|index| {
            let red_index = index >> 10;
            let green_index = (index >> 5) & 0x1f;
            let blue_index = index & 0x1f;
            let target_red = (red_index * 8 + 4) as f32;
            let target_green = (green_index * 8 + 4) as f32;
            let target_blue = (blue_index * 8 + 4) as f32;

            let mut weighted_red = 0.0;
            let mut weighted_green = 0.0;
            let mut weighted_blue = 0.0;
            let mut weight_sum = 0.0;

            for &(red, green, blue) in &palette {
                let red_delta = target_red - red;
                let green_delta = target_green - green;
                let blue_delta = target_blue - blue;
                let distance_squared =
                    red_delta * red_delta + green_delta * green_delta + blue_delta * blue_delta;
                let weight = f64::from((-distance_squared / two_sigma_squared).exp());
                weighted_red += f64::from(red) * weight;
                weighted_green += f64::from(green) * weight;
                weighted_blue += f64::from(blue) * weight;
                weight_sum += weight;
            }

            let inverse_weight = 1.0 / weight_sum.max(1e-30);
            [
                (weighted_red * inverse_weight).clamp(0.0, 255.0) as u8,
                (weighted_green * inverse_weight).clamp(0.0, 255.0) as u8,
                (weighted_blue * inverse_weight).clamp(0.0, 255.0) as u8,
            ]
        })
        .collect()
}
