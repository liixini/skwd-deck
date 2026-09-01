#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]

use image::DynamicImage;
use rayon::prelude::*;
use serde_json::{Value, json};

use super::imgutil::{clampu8, f64_param, i64_param, luma, str_param};
use super::registry::EffectDef;

pub fn effects() -> Vec<EffectDef> {
    vec![
        EffectDef {
            id: "posterize",
            schema: posterize_schema,
            render: |img, params| Ok(apply_posterize(img, params)),
        },
        EffectDef {
            id: "solarize",
            schema: solarize_schema,
            render: |img, params| Ok(apply_solarize(img, params)),
        },
        EffectDef {
            id: "threshold",
            schema: threshold_schema,
            render: |img, params| Ok(apply_threshold(img, params)),
        },
        EffectDef {
            id: "vignette",
            schema: vignette_schema,
            render: |img, params| Ok(apply_vignette(img, params)),
        },
        EffectDef {
            id: "dim",
            schema: dim_schema,
            render: |img, params| Ok(apply_dim(img, params)),
        },
    ]
}

fn posterize_schema() -> Value {
    json!({
        "id": "posterize", "label": "Posterize",
        "description": "Use fewer colour levels. Fewer levels means flatter colours.",
        "params": [
            { "id": "levels", "label": "Levels", "type": "integer",
              "min": 2, "max": 32, "step": 1, "default": 6 }
        ]
    })
}

fn apply_posterize(img: DynamicImage, params: &Value) -> DynamicImage {
    let levels = i64_param(params, "levels", 6).clamp(2, 256) as u32;
    let mut lut = [0u8; 256];
    let step = 255.0 / (levels - 1) as f32;
    for (val, slot) in lut.iter_mut().enumerate() {
        let quant = ((val as f32 / 255.0 * (levels - 1) as f32).round() * step).round();
        *slot = clampu8(quant);
    }
    let mut rgba = img.into_rgba8();
    rgba.par_chunks_exact_mut(4).for_each(|px| {
        px[0] = lut[px[0] as usize];
        px[1] = lut[px[1] as usize];
        px[2] = lut[px[2] as usize];
    });
    DynamicImage::ImageRgba8(rgba)
}

fn solarize_schema() -> Value {
    json!({
        "id": "solarize", "label": "Solarize",
        "description": "Invert each colour channel once it passes the threshold.",
        "params": [
            { "id": "threshold", "label": "Threshold", "type": "integer",
              "min": 0, "max": 255, "step": 1, "default": 128 }
        ]
    })
}

fn apply_solarize(img: DynamicImage, params: &Value) -> DynamicImage {
    let thr = i64_param(params, "threshold", 128).clamp(0, 255) as u8;
    let mut rgba = img.into_rgba8();
    rgba.par_chunks_exact_mut(4).for_each(|px| {
        for ch in px.iter_mut().take(3) {
            if *ch >= thr {
                *ch = 255 - *ch;
            }
        }
    });
    DynamicImage::ImageRgba8(rgba)
}

fn threshold_schema() -> Value {
    json!({
        "id": "threshold", "label": "Threshold",
        "description": "Turn every pixel black or white. Cutoff decides where it changes.",
        "params": [
            { "id": "cutoff", "label": "Cutoff", "type": "integer",
              "min": 0, "max": 255, "step": 1, "default": 128 }
        ]
    })
}

fn apply_threshold(img: DynamicImage, params: &Value) -> DynamicImage {
    let cutoff = i64_param(params, "cutoff", 128).clamp(0, 255) as f32;
    let mut rgba = img.into_rgba8();
    rgba.par_chunks_exact_mut(4).for_each(|px| {
        let val = if luma(px[0], px[1], px[2]) >= cutoff { 255 } else { 0 };
        px[0] = val;
        px[1] = val;
        px[2] = val;
    });
    DynamicImage::ImageRgba8(rgba)
}

fn vignette_schema() -> Value {
    json!({
        "id": "vignette", "label": "Vignette",
        "description": "Darken the outside of the image. Radius decides where the darkening starts.",
        "params": [
            { "id": "strength", "label": "Strength", "type": "number",
              "min": 0.0, "max": 1.0, "step": 0.05, "decimals": 2, "default": 0.6 },
            { "id": "radius", "label": "Radius", "type": "number",
              "min": 0.1, "max": 1.5, "step": 0.05, "decimals": 2, "default": 0.7 }
        ]
    })
}

fn apply_vignette(img: DynamicImage, params: &Value) -> DynamicImage {
    let strength = f64_param(params, "strength", 0.6).clamp(0.0, 1.0) as f32;
    let radius = f64_param(params, "radius", 0.7).clamp(0.05, 2.0) as f32;
    let rgba = img.into_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let maxd = (cx * cx + cy * cy).sqrt();
    let mut raw = rgba.into_raw();
    raw.par_chunks_exact_mut(4).enumerate().for_each(|(idx, px)| {
        let x = (idx as u32 % w) as f32;
        let y = (idx as u32 / w) as f32;
        let dist = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() / maxd;
        let fade = ((dist - radius) / (1.0 - radius).max(1e-3)).clamp(0.0, 1.0);
        let dark = 1.0 - fade * strength;
        px[0] = clampu8(f32::from(px[0]) * dark);
        px[1] = clampu8(f32::from(px[1]) * dark);
        px[2] = clampu8(f32::from(px[2]) * dark);
    });
    DynamicImage::ImageRgba8(image::RgbaImage::from_raw(w, h, raw).unwrap())
}

fn dim_schema() -> Value {
    json!({
        "id": "dim", "label": "Edge darken",
        "description": "Darken the top, bottom, or both. Size controls how far it reaches.",
        "params": [
            { "id": "edge", "label": "Edge", "type": "dropdown", "default": "top",
              "options": [
                  { "mode": "top",    "label": "Top" },
                  { "mode": "bottom", "label": "Bottom" },
                  { "mode": "both",   "label": "Top + bottom" }
              ] },
            { "id": "strength", "label": "Strength", "type": "number",
              "min": 0.0, "max": 1.0, "step": 0.05, "decimals": 2, "default": 0.5 },
            { "id": "size", "label": "Size", "type": "integer",
              "min": 5, "max": 60, "step": 1, "default": 25 }
        ]
    })
}

fn apply_dim(img: DynamicImage, params: &Value) -> DynamicImage {
    let edge = str_param(params, "edge", "top").to_string();
    let strength = f64_param(params, "strength", 0.5).clamp(0.0, 1.0) as f32;
    let size = (i64_param(params, "size", 25).clamp(1, 100) as f32) / 100.0;
    let rgba = img.into_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let band = (h as f32 * size).max(1.0);
    let mut raw = rgba.into_raw();
    raw.par_chunks_exact_mut(4).enumerate().for_each(|(idx, px)| {
        let y = (idx as u32 / w) as f32;
        let top_f = ((band - y) / band).clamp(0.0, 1.0);
        let bot_f = ((y - (h as f32 - band)) / band).clamp(0.0, 1.0);
        let fade = match edge.as_str() {
            "bottom" => bot_f,
            "both" => top_f.max(bot_f),
            _ => top_f,
        };
        let dark = 1.0 - fade * strength;
        px[0] = clampu8(f32::from(px[0]) * dark);
        px[1] = clampu8(f32::from(px[1]) * dark);
        px[2] = clampu8(f32::from(px[2]) * dark);
    });
    DynamicImage::ImageRgba8(image::RgbaImage::from_raw(w, h, raw).unwrap())
}
