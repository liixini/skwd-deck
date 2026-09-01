#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap
)]

use image::{DynamicImage, RgbaImage};
use rayon::prelude::*;
use serde_json::{Value, json};

use super::imgutil::{clampu8, f64_param, hash01, i64_param, luma};
use super::registry::EffectDef;

pub fn effects() -> Vec<EffectDef> {
    vec![
        EffectDef {
            id: "scanlines",
            schema: scanlines_schema,
            render: |img, params| Ok(apply_scanlines(img, params)),
        },
        EffectDef {
            id: "grain",
            schema: grain_schema,
            render: |img, params| Ok(apply_grain(img, params)),
        },
        EffectDef {
            id: "sharpen",
            schema: sharpen_schema,
            render: |img, params| Ok(apply_convolution(img, params, Kernel::Sharpen)),
        },
        EffectDef {
            id: "emboss",
            schema: emboss_schema,
            render: |img, _| Ok(apply_convolution(img, &Value::Null, Kernel::Emboss)),
        },
        EffectDef {
            id: "edge",
            schema: edge_schema,
            render: |img, params| Ok(apply_edge(img, params)),
        },
        EffectDef {
            id: "halftone",
            schema: halftone_schema,
            render: |img, params| Ok(apply_halftone(img, params)),
        },
    ]
}

fn scanlines_schema() -> Value {
    json!({
        "id": "scanlines", "label": "Scanlines",
        "description": "Add dark horizontal lines. Spacing controls how far apart they are.",
        "params": [
            { "id": "spacing", "label": "Spacing", "type": "integer",
              "min": 2, "max": 20, "step": 1, "default": 3 },
            { "id": "intensity", "label": "Intensity", "type": "number",
              "min": 0.0, "max": 1.0, "step": 0.05, "decimals": 2, "default": 0.4 }
        ]
    })
}

fn apply_scanlines(img: DynamicImage, params: &Value) -> DynamicImage {
    let spacing = i64_param(params, "spacing", 3).max(2) as u32;
    let intensity = f64_param(params, "intensity", 0.4).clamp(0.0, 1.0) as f32;
    let rgba = img.into_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let mut raw = rgba.into_raw();
    raw.par_chunks_exact_mut(4).enumerate().for_each(|(idx, px)| {
        let y = idx as u32 / w;
        if y.is_multiple_of(spacing) {
            let dark = 1.0 - intensity;
            px[0] = clampu8(f32::from(px[0]) * dark);
            px[1] = clampu8(f32::from(px[1]) * dark);
            px[2] = clampu8(f32::from(px[2]) * dark);
        }
    });
    DynamicImage::ImageRgba8(RgbaImage::from_raw(w, h, raw).unwrap())
}

fn grain_schema() -> Value {
    json!({
        "id": "grain", "label": "Film grain",
        "description": "Add black-and-white noise. Move the slider right for more grain.",
        "params": [
            { "id": "amount", "label": "Grain", "type": "integer",
              "min": 1, "max": 100, "step": 1, "default": 25 }
        ]
    })
}

fn apply_grain(img: DynamicImage, params: &Value) -> DynamicImage {
    let amount = i64_param(params, "amount", 25).clamp(0, 255) as f32;
    let rgba = img.into_rgba8();
    let w = rgba.width();
    let h = rgba.height();
    let mut raw = rgba.into_raw();
    raw.par_chunks_exact_mut(4).enumerate().for_each(|(idx, px)| {
        let noise = (hash01(idx as u32) - 0.5) * 2.0 * amount;
        px[0] = clampu8(f32::from(px[0]) + noise);
        px[1] = clampu8(f32::from(px[1]) + noise);
        px[2] = clampu8(f32::from(px[2]) + noise);
    });
    DynamicImage::ImageRgba8(RgbaImage::from_raw(w, h, raw).unwrap())
}

#[derive(Clone, Copy)]
enum Kernel {
    Sharpen,
    Emboss,
}

fn sharpen_schema() -> Value {
    json!({
        "id": "sharpen", "label": "Sharpen",
        "description": "Make edges stand out. Move the slider right for harder edges.",
        "params": [
            { "id": "amount", "label": "Sharpness", "type": "number",
              "min": 0.0, "max": 3.0, "step": 0.1, "decimals": 1, "default": 1.0 }
        ]
    })
}

fn emboss_schema() -> Value {
    json!({ "id": "emboss", "label": "Emboss", "description": "Turn the image into raised grey edges.", "params": [] })
}

fn apply_convolution(img: DynamicImage, params: &Value, kernel: Kernel) -> DynamicImage {
    let amount = f64_param(params, "amount", 1.0).clamp(0.0, 5.0) as f32;
    let kern: [f32; 9] = match kernel {
        Kernel::Sharpen => {
            let center = 1.0 + 4.0 * amount;
            let edge = -amount;
            [0.0, edge, 0.0, edge, center, edge, 0.0, edge, 0.0]
        }
        Kernel::Emboss => [-2.0, -1.0, 0.0, -1.0, 1.0, 1.0, 0.0, 1.0, 2.0],
    };
    let bias = matches!(kernel, Kernel::Emboss);
    let src = img.into_rgba8();
    let (w, h) = (src.width(), src.height());
    let mut out = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let px = out.get_pixel_mut(x, y);
            for ch in 0..3 {
                px[ch] = conv_px(&src, x, y, ch, &kern, bias);
            }
            px[3] = src.get_pixel(x, y)[3];
        }
    }
    DynamicImage::ImageRgba8(out)
}

fn conv_px(src: &RgbaImage, x: u32, y: u32, ch: usize, kern: &[f32; 9], bias: bool) -> u8 {
    let (w, h) = (src.width(), src.height());
    let getp = |x: i64, y: i64| -> f32 {
        let xx = x.clamp(0, i64::from(w) - 1) as u32;
        let yy = y.clamp(0, i64::from(h) - 1) as u32;
        f32::from(src.get_pixel(xx, yy)[ch])
    };
    let mut acc = 0.0;
    let mut ki = 0;
    for dy in -1i64..=1 {
        for dx in -1i64..=1 {
            acc += getp(i64::from(x) + dx, i64::from(y) + dy) * kern[ki];
            ki += 1;
        }
    }
    if bias {
        acc += 128.0;
    }
    clampu8(acc)
}

fn edge_schema() -> Value {
    json!({
        "id": "edge", "label": "Edge detect",
        "description": "Show edges on black. A lower threshold keeps more detail.",
        "params": [
            { "id": "threshold", "label": "Threshold", "type": "integer",
              "min": 0, "max": 255, "step": 1, "default": 60 }
        ]
    })
}

fn apply_edge(img: DynamicImage, params: &Value) -> DynamicImage {
    let thr = i64_param(params, "threshold", 60).clamp(0, 255) as f32;
    let src = img.into_rgba8();
    let (w, h) = (src.width(), src.height());
    let lum = |x: i64, y: i64| -> f32 {
        let xx = x.clamp(0, i64::from(w) - 1) as u32;
        let yy = y.clamp(0, i64::from(h) - 1) as u32;
        let px = src.get_pixel(xx, yy);
        luma(px[0], px[1], px[2])
    };
    let mut out = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let (xi, yi) = (i64::from(x), i64::from(y));
            let gx = -lum(xi - 1, yi - 1) - 2.0 * lum(xi - 1, yi) - lum(xi - 1, yi + 1)
                + lum(xi + 1, yi - 1)
                + 2.0 * lum(xi + 1, yi)
                + lum(xi + 1, yi + 1);
            let gy = -lum(xi - 1, yi - 1) - 2.0 * lum(xi, yi - 1) - lum(xi + 1, yi - 1)
                + lum(xi - 1, yi + 1)
                + 2.0 * lum(xi, yi + 1)
                + lum(xi + 1, yi + 1);
            let mag = (gx * gx + gy * gy).sqrt();
            let val = if mag >= thr { clampu8(mag) } else { 0 };
            out.put_pixel(x, y, image::Rgba([val, val, val, src.get_pixel(x, y)[3]]));
        }
    }
    DynamicImage::ImageRgba8(out)
}

fn halftone_schema() -> Value {
    json!({
        "id": "halftone", "label": "Halftone",
        "description": "Turn the image into black dots. Dot size controls the spacing as well.",
        "params": [
            { "id": "size", "label": "Dot size", "type": "integer",
              "min": 3, "max": 40, "step": 1, "default": 8 }
        ]
    })
}

fn apply_halftone(img: DynamicImage, params: &Value) -> DynamicImage {
    let cell = i64_param(params, "size", 8).max(2) as u32;
    let src = img.into_rgba8();
    let (w, h) = (src.width(), src.height());
    let mut out = RgbaImage::from_pixel(w, h, image::Rgba([255, 255, 255, 255]));
    let mut cy = 0;
    while cy < h {
        let mut cx = 0;
        while cx < w {
            let avg = cell_avg_luma(&src, cx, cy, cell);
            let darkness = 1.0 - avg / 255.0;
            let max_r = cell as f32 / 2.0;
            let rad = darkness.sqrt() * max_r;
            stamp_dot(&mut out, cx, cy, cell, rad);
            cx += cell;
        }
        cy += cell;
    }
    DynamicImage::ImageRgba8(out)
}

fn cell_avg_luma(src: &RgbaImage, cx: u32, cy: u32, cell: u32) -> f32 {
    let (w, h) = (src.width(), src.height());
    let mut sum = 0.0f32;
    let mut count = 0.0f32;
    for y in cy..(cy + cell).min(h) {
        for x in cx..(cx + cell).min(w) {
            let px = src.get_pixel(x, y);
            sum += luma(px[0], px[1], px[2]);
            count += 1.0;
        }
    }
    if count > 0.0 { sum / count } else { 255.0 }
}

fn stamp_dot(out: &mut RgbaImage, cx: u32, cy: u32, cell: u32, rad: f32) {
    let (w, h) = (out.width(), out.height());
    let ccx = cx as f32 + cell as f32 / 2.0;
    let ccy = cy as f32 + cell as f32 / 2.0;
    for y in cy..(cy + cell).min(h) {
        for x in cx..(cx + cell).min(w) {
            let dist = ((x as f32 - ccx).powi(2) + (y as f32 - ccy).powi(2)).sqrt();
            if dist <= rad {
                out.put_pixel(x, y, image::Rgba([20, 20, 20, 255]));
            }
        }
    }
}
