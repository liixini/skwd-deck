use image::{DynamicImage, Rgba, RgbaImage};
use serde_json::Value;

use super::theme;
use crate::imgutil::i64_param;

pub(crate) fn render(
    effect: &str,
    image: DynamicImage,
    params: &Value,
) -> anyhow::Result<DynamicImage> {
    match effect {
        "theme" => theme::apply(image, params),
        "invert" => Ok(apply_invert(image)),
        "flip" => Ok(image::imageops::flip_vertical(&image).into()),
        "mirror" => Ok(image::imageops::flip_horizontal(&image).into()),
        "grayscale" => Ok(image.grayscale()),
        "brightness" => Ok(apply_brightness(image, params)),
        "contrast" => Ok(apply_contrast(image, params)),
        "saturation" => Ok(apply_saturation(image, params)),
        "gamma" => Ok(apply_gamma(image, params)),
        "pixelate" => Ok(apply_pixelate(&image, params)),
        "border" => Ok(apply_border(image, params)),
        "round" => Ok(apply_round(image, params)),
        other => anyhow::bail!("unknown effect: {other}"),
    }
}

fn apply_invert(mut image: DynamicImage) -> DynamicImage {
    image.invert();
    image
}

fn apply_brightness(image: DynamicImage, params: &Value) -> DynamicImage {
    let factor = params.get("factor").and_then(Value::as_f64).unwrap_or(1.1) as f32;
    let mut rgba = image.into_rgba8();
    for pixel in rgba.pixels_mut() {
        for channel in 0..3 {
            pixel[channel] = (f32::from(pixel[channel]) * factor).clamp(0.0, 255.0) as u8;
        }
    }
    DynamicImage::ImageRgba8(rgba)
}

pub(crate) fn apply_gamma(image: DynamicImage, params: &Value) -> DynamicImage {
    let gamma = params.get("gamma").and_then(Value::as_f64).unwrap_or(1.0).max(0.001) as f32;
    let inverse = 1.0 / gamma;
    let mut lookup = [0; 256];
    for (value, slot) in lookup.iter_mut().enumerate() {
        let normalized = (value as f32 / 255.0).powf(inverse);
        *slot = (normalized * 255.0).clamp(0.0, 255.0) as u8;
    }
    let mut rgba = image.into_rgba8();
    for pixel in rgba.pixels_mut() {
        pixel[0] = lookup[pixel[0] as usize];
        pixel[1] = lookup[pixel[1] as usize];
        pixel[2] = lookup[pixel[2] as usize];
    }
    DynamicImage::ImageRgba8(rgba)
}

pub(crate) fn apply_contrast(image: DynamicImage, params: &Value) -> DynamicImage {
    let mode = params.get("mode").and_then(Value::as_str).unwrap_or("normal");
    let factor = params.get("factor").and_then(Value::as_f64).unwrap_or(25.0) as f32;
    let mut lookup = [0; 256];

    if mode == "sigmoid" {
        let k = (factor / 25.0).clamp(-8.0, 8.0);
        let denominator_low = 1.0 + (-k * (-0.5) * 2.0_f32).exp();
        let denominator_high = 1.0 + (-k * 0.5 * 2.0_f32).exp();
        let sigmoid_low = 1.0 / denominator_low;
        let sigmoid_high = 1.0 / denominator_high;
        let span = (sigmoid_high - sigmoid_low).abs().max(1e-6);
        for (value, slot) in lookup.iter_mut().enumerate() {
            let normalized = value as f32 / 255.0;
            let sigmoid = 1.0 / (1.0 + (-k * (normalized - 0.5) * 2.0_f32).exp());
            let scaled = (sigmoid - sigmoid_low) / span;
            *slot = (scaled * 255.0).clamp(0.0, 255.0) as u8;
        }
    } else {
        let gain = ((factor + 100.0) / 100.0).max(0.0);
        for (value, slot) in lookup.iter_mut().enumerate() {
            let output = (value as f32 - 127.5) * gain + 127.5;
            *slot = output.clamp(0.0, 255.0) as u8;
        }
    }

    let mut rgba = image.into_rgba8();
    for pixel in rgba.pixels_mut() {
        pixel[0] = lookup[pixel[0] as usize];
        pixel[1] = lookup[pixel[1] as usize];
        pixel[2] = lookup[pixel[2] as usize];
    }
    DynamicImage::ImageRgba8(rgba)
}

fn apply_saturation(image: DynamicImage, params: &Value) -> DynamicImage {
    let percentage = i64_param(params, "percentage", 25) as f32;
    let factor = 1.0 + percentage / 100.0;
    let mut rgba = image.into_rgba8();
    for pixel in rgba.pixels_mut() {
        let red = f32::from(pixel[0]);
        let green = f32::from(pixel[1]);
        let blue = f32::from(pixel[2]);
        let luminance = 0.299 * red + 0.587 * green + 0.114 * blue;
        pixel[0] = (luminance + (red - luminance) * factor).clamp(0.0, 255.0) as u8;
        pixel[1] = (luminance + (green - luminance) * factor).clamp(0.0, 255.0) as u8;
        pixel[2] = (luminance + (blue - luminance) * factor).clamp(0.0, 255.0) as u8;
    }
    DynamicImage::ImageRgba8(rgba)
}

pub(crate) fn apply_pixelate(image: &DynamicImage, params: &Value) -> DynamicImage {
    let scale = i64_param(params, "scale", 15).max(2) as u32;
    let (width, height) = (image.width(), image.height());
    let small_width = (width / scale).max(1);
    let small_height = (height / scale).max(1);
    let small = image::imageops::resize(
        image,
        small_width,
        small_height,
        image::imageops::FilterType::Triangle,
    );
    let output =
        image::imageops::resize(&small, width, height, image::imageops::FilterType::Nearest);
    DynamicImage::ImageRgba8(output)
}

pub(crate) fn apply_border(image: DynamicImage, params: &Value) -> DynamicImage {
    let color = params.get("color").and_then(Value::as_str).unwrap_or("#1a1a1a");
    let thickness = i64_param(params, "thickness", 30).max(0) as u32;
    let radius = i64_param(params, "radius", 0).max(0) as u32;

    let (red, green, blue) = crate::imgutil::parse_hex_argb(color).unwrap_or((26, 26, 26));
    let (width, height) = (image.width(), image.height());
    let mut output = RgbaImage::from_pixel(
        width + thickness * 2,
        height + thickness * 2,
        Rgba([red, green, blue, 255]),
    );
    image::imageops::overlay(
        &mut output,
        &image.into_rgba8(),
        i64::from(thickness),
        i64::from(thickness),
    );
    if radius > 0 {
        apply_corner_mask(&mut output, radius);
    }
    DynamicImage::ImageRgba8(output)
}

pub(crate) fn apply_round(image: DynamicImage, params: &Value) -> DynamicImage {
    let radius = i64_param(params, "radius", 60).max(1) as u32;
    let mut rgba = image.into_rgba8();
    apply_corner_mask(&mut rgba, radius);
    DynamicImage::ImageRgba8(rgba)
}

fn apply_corner_mask(image: &mut RgbaImage, radius: u32) {
    let (width, height) = (image.width(), image.height());
    let radius = radius.min(width / 2).min(height / 2);
    if radius == 0 {
        return;
    }
    let radius_float = radius as f32;

    let corners = [
        (0, 0, radius_float, radius_float),
        (width - radius, 0, (width - radius) as f32, radius_float),
        (0, height - radius, radius_float, (height - radius) as f32),
        (width - radius, height - radius, (width - radius) as f32, (height - radius) as f32),
    ];

    for &(origin_x, origin_y, center_x, center_y) in &corners {
        for delta_y in 0..radius {
            for delta_x in 0..radius {
                let x = origin_x + delta_x;
                let y = origin_y + delta_y;
                let alpha =
                    corner_alpha(x as f32 + 0.5, y as f32 + 0.5, center_x, center_y, radius_float);
                if alpha < 1.0 {
                    let pixel = image.get_pixel_mut(x, y);
                    pixel[3] = (f32::from(pixel[3]) * alpha) as u8;
                }
            }
        }
    }
}

#[allow(clippy::inline_always)]
#[inline(always)]
fn corner_alpha(x: f32, y: f32, center_x: f32, center_y: f32, radius: f32) -> f32 {
    let distance = ((x - center_x).powi(2) + (y - center_y).powi(2)).sqrt();
    if distance <= radius - 0.5 {
        1.0
    } else if distance >= radius + 0.5 {
        0.0
    } else {
        radius + 0.5 - distance
    }
}
