#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap
)]

use image::{DynamicImage, RgbaImage};
use rayon::prelude::*;
use serde_json::{Value, json};

use super::imgutil::{
    clampu8, color_param, f64_param, gradient_sample, hsl_to_rgb, i64_param, lerp, luma,
    rgb_to_hsl, str_param,
};
use super::registry::EffectDef;
use skwd_palette::gowall as themes;

pub fn effects() -> Vec<EffectDef> {
    vec![
        EffectDef {
            id: "duotone",
            schema: duotone_schema,
            render: |img, params| Ok(apply_duotone(img, params)),
        },
        EffectDef { id: "gradientmap", schema: gradientmap_schema, render: apply_gradientmap },
        EffectDef {
            id: "sepia",
            schema: sepia_schema,
            render: |img, params| Ok(apply_sepia(img, params)),
        },
        EffectDef {
            id: "hue",
            schema: hue_schema,
            render: |img, params| Ok(apply_hue(img, params)),
        },
        EffectDef {
            id: "temperature",
            schema: temperature_schema,
            render: |img, params| Ok(apply_temperature(img, params)),
        },
        EffectDef {
            id: "tint",
            schema: tint_schema,
            render: |img, params| Ok(apply_tint(img, params)),
        },
        EffectDef {
            id: "chromatic",
            schema: chromatic_schema,
            render: |img, params| Ok(apply_chromatic(img, params)),
        },
    ]
}

fn duotone_schema() -> Value {
    json!({
        "id": "duotone", "label": "Duotone",
        "description": "Use one colour for dark areas and another for light areas.",
        "params": [
            { "id": "shadow",    "label": "Shadow",    "type": "color", "default": "#1e1e2e" },
            { "id": "highlight", "label": "Highlight", "type": "color", "default": "#cdd6f4" }
        ]
    })
}

fn apply_duotone(img: DynamicImage, params: &Value) -> DynamicImage {
    let lo = color_param(params, "shadow", "#1e1e2e");
    let hi = color_param(params, "highlight", "#cdd6f4");
    let mut rgba = img.into_rgba8();
    rgba.par_chunks_exact_mut(4).for_each(|px| {
        let t = luma(px[0], px[1], px[2]) / 255.0;
        px[0] = lerp(lo.0, hi.0, t);
        px[1] = lerp(lo.1, hi.1, t);
        px[2] = lerp(lo.2, hi.2, t);
    });
    DynamicImage::ImageRgba8(rgba)
}

fn gradientmap_schema() -> Value {
    json!({
        "id": "gradientmap", "label": "Gradient map",
        "description": "Pick a colour from the palette based on how bright each pixel is.",
        "params": [
            { "id": "theme", "label": "Palette", "type": "dropdown",
              "default": themes::names().first().copied().unwrap_or("Catppuccin"),
              "options": themes::options() }
        ]
    })
}

fn apply_gradientmap(img: DynamicImage, params: &Value) -> anyhow::Result<DynamicImage> {
    let name = str_param(params, "theme", "Catppuccin");
    let mut palette =
        themes::lookup(name).ok_or_else(|| anyhow::anyhow!("unknown theme: {name}"))?.to_vec();
    if palette.is_empty() {
        anyhow::bail!("theme {name} has no colours");
    }
    palette.sort_by(|a, b| luma(a.0, a.1, a.2).total_cmp(&luma(b.0, b.1, b.2)));
    let mut rgba = img.into_rgba8();
    rgba.par_chunks_exact_mut(4).for_each(|px| {
        let t = luma(px[0], px[1], px[2]) / 255.0;
        let (r, g, b) = gradient_sample(&palette, t);
        px[0] = r;
        px[1] = g;
        px[2] = b;
    });
    Ok(DynamicImage::ImageRgba8(rgba))
}

fn sepia_schema() -> Value {
    json!({
        "id": "sepia", "label": "Sepia",
        "description": "Mix a sepia tone into the image. Move the slider right for more.",
        "params": [
            { "id": "intensity", "label": "Intensity", "type": "number",
              "min": 0.0, "max": 1.0, "step": 0.05, "decimals": 2, "default": 1.0 }
        ]
    })
}

fn apply_sepia(img: DynamicImage, params: &Value) -> DynamicImage {
    let amt = f64_param(params, "intensity", 1.0).clamp(0.0, 1.0) as f32;
    let mut rgba = img.into_rgba8();
    rgba.par_chunks_exact_mut(4).for_each(|px| {
        let (r, g, b) = (f32::from(px[0]), f32::from(px[1]), f32::from(px[2]));
        let sr = 0.393 * r + 0.769 * g + 0.189 * b;
        let sg = 0.349 * r + 0.686 * g + 0.168 * b;
        let sb = 0.272 * r + 0.534 * g + 0.131 * b;
        px[0] = clampu8(r + (sr - r) * amt);
        px[1] = clampu8(g + (sg - g) * amt);
        px[2] = clampu8(b + (sb - b) * amt);
    });
    DynamicImage::ImageRgba8(rgba)
}

fn hue_schema() -> Value {
    json!({
        "id": "hue", "label": "Hue",
        "description": "Move every colour around the colour wheel.",
        "params": [
            { "id": "degrees", "label": "Degrees", "type": "integer",
              "min": 0, "max": 360, "step": 1, "default": 180 }
        ]
    })
}

fn apply_hue(img: DynamicImage, params: &Value) -> DynamicImage {
    let shift = (i64_param(params, "degrees", 180) as f32) / 360.0;
    let mut rgba = img.into_rgba8();
    rgba.par_chunks_exact_mut(4).for_each(|px| {
        let (h, s, l) = rgb_to_hsl(px[0], px[1], px[2]);
        let (r, g, b) = hsl_to_rgb((h + shift).rem_euclid(1.0), s, l);
        px[0] = r;
        px[1] = g;
        px[2] = b;
    });
    DynamicImage::ImageRgba8(rgba)
}

fn temperature_schema() -> Value {
    json!({
        "id": "temperature", "label": "Colour temperature",
        "description": "Move the image between cooler and warmer colours.",
        "params": [
            { "id": "amount", "label": "Temperature", "type": "integer",
              "min": -100, "max": 100, "step": 1, "default": 30 }
        ]
    })
}

fn apply_temperature(img: DynamicImage, params: &Value) -> DynamicImage {
    let amt = (i64_param(params, "amount", 30) as f32) / 100.0;
    let rd = amt * 50.0;
    let bd = -amt * 50.0;
    let mut rgba = img.into_rgba8();
    rgba.par_chunks_exact_mut(4).for_each(|px| {
        px[0] = clampu8(f32::from(px[0]) + rd);
        px[2] = clampu8(f32::from(px[2]) + bd);
    });
    DynamicImage::ImageRgba8(rgba)
}

fn tint_schema() -> Value {
    json!({
        "id": "tint", "label": "Tint",
        "description": "Lay one colour over the image. Opacity controls how much colour is added.",
        "params": [
            { "id": "color",   "label": "Colour",  "type": "color", "default": "#7aa2f7" },
            { "id": "opacity", "label": "Opacity", "type": "number",
              "min": 0.0, "max": 1.0, "step": 0.05, "decimals": 2, "default": 0.35 }
        ]
    })
}

fn apply_tint(img: DynamicImage, params: &Value) -> DynamicImage {
    let (tr, tg, tb) = color_param(params, "color", "#7aa2f7");
    let a = f64_param(params, "opacity", 0.35).clamp(0.0, 1.0) as f32;
    let mut rgba = img.into_rgba8();
    rgba.par_chunks_exact_mut(4).for_each(|px| {
        px[0] = lerp(px[0], tr, a);
        px[1] = lerp(px[1], tg, a);
        px[2] = lerp(px[2], tb, a);
    });
    DynamicImage::ImageRgba8(rgba)
}

fn chromatic_schema() -> Value {
    json!({
        "id": "chromatic", "label": "Chromatic aberration",
        "description": "Pull the red and blue channels apart. Offset controls the distance.",
        "params": [
            { "id": "offset", "label": "Offset", "type": "integer",
              "min": 1, "max": 80, "step": 1, "default": 8 }
        ]
    })
}

fn apply_chromatic(img: DynamicImage, params: &Value) -> DynamicImage {
    let off = i64_param(params, "offset", 8).max(0);
    let src = img.into_rgba8();
    let (w, h) = (src.width(), src.height());
    let mut out = RgbaImage::new(w, h);
    let sample = |x: i64, y: i64, ch: usize| -> u8 {
        let xx = x.clamp(0, i64::from(w) - 1) as u32;
        let yy = y.clamp(0, i64::from(h) - 1) as u32;
        src.get_pixel(xx, yy)[ch]
    };
    for y in 0..h {
        for x in 0..w {
            let (xi, yi) = (i64::from(x), i64::from(y));
            let px = out.get_pixel_mut(x, y);
            px[0] = sample(xi + off, yi, 0);
            px[1] = src.get_pixel(x, y)[1];
            px[2] = sample(xi - off, yi, 2);
            px[3] = src.get_pixel(x, y)[3];
        }
    }
    DynamicImage::ImageRgba8(out)
}
