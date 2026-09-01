#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap
)]

use image::{DynamicImage, RgbaImage};
use rayon::prelude::*;
use serde_json::{Value, json};

use super::imgutil::{f64_param, hash01, i64_param};
use super::registry::EffectDef;

pub fn effects() -> Vec<EffectDef> {
    vec![
        EffectDef {
            id: "blur",
            schema: blur_schema,
            render: |img, params| Ok(apply_blur(img, params)),
        },
        EffectDef {
            id: "glitch",
            schema: glitch_schema,
            render: |img, params| Ok(apply_glitch(img, params)),
        },
        EffectDef {
            id: "kaleidoscope",
            schema: kaleidoscope_schema,
            render: |img, params| Ok(apply_kaleidoscope(img, params)),
        },
        EffectDef {
            id: "kuwahara",
            schema: kuwahara_schema,
            render: |img, params| Ok(apply_kuwahara(img, params)),
        },
    ]
}

fn blur_schema() -> Value {
    json!({
        "id": "blur", "label": "Blur",
        "description": "Blur the image. Move the slider right for more blur.",
        "params": [
            { "id": "radius", "label": "Blur radius", "type": "number",
              "min": 0.5, "max": 50.0, "step": 0.5, "decimals": 1, "default": 8.0 }
        ]
    })
}

fn apply_blur(img: DynamicImage, params: &Value) -> DynamicImage {
    let sigma = f64_param(params, "radius", 8.0).clamp(0.1, 100.0) as f32;
    DynamicImage::ImageRgba8(image::imageops::blur(&img.into_rgba8(), sigma))
}

fn glitch_schema() -> Value {
    json!({
        "id": "glitch", "label": "Glitch",
        "description": "Shift horizontal bands and pull the colour channels apart. Intensity controls how far.",
        "params": [
            { "id": "intensity", "label": "Intensity", "type": "integer",
              "min": 1, "max": 100, "step": 1, "default": 40 },
            { "id": "seed", "label": "Seed", "type": "integer",
              "min": 0, "max": 9999, "step": 1, "default": 7 }
        ]
    })
}

fn apply_glitch(img: DynamicImage, params: &Value) -> DynamicImage {
    let intensity = i64_param(params, "intensity", 40).clamp(0, 100) as f32 / 100.0;
    let seed = i64_param(params, "seed", 7) as u32;
    let src = img.into_rgba8();
    let (w, h) = (src.width(), src.height());
    let max_shift = (w as f32 * 0.15 * intensity).max(1.0);
    let band_h = (8.0 + 40.0 * (1.0 - intensity)).max(2.0) as u32;
    let chan = (max_shift * 0.4) as i64;
    let mut out = RgbaImage::new(w, h);
    for y in 0..h {
        let band = y / band_h;
        let rand = hash01(seed.wrapping_mul(2_654_435_761).wrapping_add(band));
        let shift =
            if rand > 0.6 { ((rand - 0.6) / 0.4 * 2.0 - 1.0) * max_shift } else { 0.0 } as i64;
        for x in 0..w {
            let sx = (i64::from(x) + shift).rem_euclid(i64::from(w)) as u32;
            let rx = (i64::from(x) + shift + chan).rem_euclid(i64::from(w)) as u32;
            let bx = (i64::from(x) + shift - chan).rem_euclid(i64::from(w)) as u32;
            let base = src.get_pixel(sx, y);
            out.put_pixel(
                x,
                y,
                image::Rgba([src.get_pixel(rx, y)[0], base[1], src.get_pixel(bx, y)[2], base[3]]),
            );
        }
    }
    DynamicImage::ImageRgba8(out)
}

fn kaleidoscope_schema() -> Value {
    json!({
        "id": "kaleidoscope", "label": "Kaleidoscope",
        "description": "Mirror the image around its centre. More segments means more repeats.",
        "params": [
            { "id": "segments", "label": "Segments", "type": "integer",
              "min": 2, "max": 24, "step": 1, "default": 6 }
        ]
    })
}

fn apply_kaleidoscope(img: DynamicImage, params: &Value) -> DynamicImage {
    let seg = i64_param(params, "segments", 6).clamp(2, 64) as f32;
    let src = img.into_rgba8();
    let (w, h) = (src.width(), src.height());
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let wedge = std::f32::consts::TAU / seg;
    let mut out = RgbaImage::new(w, h);
    let raw: Vec<image::Rgba<u8>> = (0..(w * h))
        .into_par_iter()
        .map(|idx| {
            let x = (idx % w) as f32;
            let y = (idx / w) as f32;
            let dx = x - cx;
            let dy = y - cy;
            let r = (dx * dx + dy * dy).sqrt();
            let mut a = dy.atan2(dx).rem_euclid(wedge);
            if a > wedge / 2.0 {
                a = wedge - a;
            }
            let sx = (cx + r * a.cos()).clamp(0.0, w as f32 - 1.0) as u32;
            let sy = (cy + r * a.sin()).clamp(0.0, h as f32 - 1.0) as u32;
            *src.get_pixel(sx, sy)
        })
        .collect();
    for (idx, px) in out.pixels_mut().enumerate() {
        *px = raw[idx];
    }
    DynamicImage::ImageRgba8(out)
}

fn kuwahara_schema() -> Value {
    json!({
        "id": "kuwahara", "label": "Kuwahara",
        "description": "Smooth colours without smearing the edges. A larger radius smooths more.",
        "params": [
            { "id": "radius", "label": "Smoothing radius", "type": "integer",
              "min": 1, "max": 10, "step": 1, "default": 4 }
        ]
    })
}

fn apply_kuwahara(img: DynamicImage, params: &Value) -> DynamicImage {
    let rad = i64_param(params, "radius", 4).clamp(1, 20);
    let src = img.into_rgba8();
    let (w, h) = (src.width(), src.height());
    let gray: Vec<f32> = src.pixels().map(|px| super::imgutil::luma(px[0], px[1], px[2])).collect();
    let at = |x: i64, y: i64| -> usize {
        let xx = x.clamp(0, i64::from(w) - 1) as u32;
        let yy = y.clamp(0, i64::from(h) - 1) as u32;
        (yy * w + xx) as usize
    };

    let raw: Vec<image::Rgba<u8>> = (0..(w * h))
        .into_par_iter()
        .map(|idx| {
            let px = i64::from(idx % w);
            let py = i64::from(idx / w);
            let quadrants = [(-rad, -rad), (0, -rad), (-rad, 0), (0, 0)];
            let mut best_var = f32::MAX;
            let mut best = [0u8; 3];
            for &(ox, oy) in &quadrants {
                let mut sum = [0.0f32; 3];
                let mut sg = 0.0f32;
                let mut sg2 = 0.0f32;
                let mut count = 0.0f32;
                for dy in 0..=rad {
                    for dx in 0..=rad {
                        let sx = px + ox + dx;
                        let sy = py + oy + dy;
                        let gidx = at(sx, sy);
                        let smp = src.get_pixel(
                            sx.clamp(0, i64::from(w) - 1) as u32,
                            sy.clamp(0, i64::from(h) - 1) as u32,
                        );
                        sum[0] += f32::from(smp[0]);
                        sum[1] += f32::from(smp[1]);
                        sum[2] += f32::from(smp[2]);
                        let lum = gray[gidx];
                        sg += lum;
                        sg2 += lum * lum;
                        count += 1.0;
                    }
                }
                let mean_g = sg / count;
                let var = sg2 / count - mean_g * mean_g;
                if var < best_var {
                    best_var = var;
                    best = [(sum[0] / count) as u8, (sum[1] / count) as u8, (sum[2] / count) as u8];
                }
            }
            image::Rgba([best[0], best[1], best[2], src.get_pixel(idx % w, idx / w)[3]])
        })
        .collect();

    let mut out = RgbaImage::new(w, h);
    for (idx, px) in out.pixels_mut().enumerate() {
        *px = raw[idx];
    }
    DynamicImage::ImageRgba8(out)
}
