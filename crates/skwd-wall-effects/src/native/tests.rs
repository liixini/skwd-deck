#![cfg(test)]

use super::*;

fn tiny(w: u32, h: u32) -> DynamicImage {
    DynamicImage::ImageRgba8(RgbaImage::from_fn(w, h, |x, y| {
        Rgba([(x * 40) as u8, (y * 60) as u8, ((x + y) * 30) as u8, 255])
    }))
}

fn detailed(w: u32, h: u32) -> DynamicImage {
    DynamicImage::ImageRgba8(RgbaImage::from_fn(w, h, |x, y| {
        Rgba([
            ((x * 17 + y * 3) % 256) as u8,
            ((x * 5 + y * 29) % 256) as u8,
            ((x * 31 + y * 11) % 256) as u8,
            255,
        ])
    }))
}

fn render(effect: &str, img: DynamicImage, params: &Value) -> anyhow::Result<DynamicImage> {
    match crate::registry::find(effect) {
        Some(def) => (def.render)(img, params),
        None => render_sync(effect, img, params),
    }
}

fn defaults(effect: &Value) -> serde_json::Map<String, Value> {
    let mut map = serde_json::Map::new();
    for param in effect["params"].as_array().unwrap() {
        if let (Some(id), Some(default)) = (param["id"].as_str(), param.get("default")) {
            map.insert(id.to_string(), default.clone());
        }
    }
    map
}

#[test]
fn effect_param_extremes() {
    for effect in list().as_array().unwrap() {
        let id = effect["id"].as_str().unwrap();
        let base = defaults(effect);
        let mut cases: Vec<serde_json::Map<String, Value>> = vec![base.clone()];
        for param in effect["params"].as_array().unwrap() {
            let param_id = param["id"].as_str().unwrap();
            for bound in ["min", "max"] {
                if let Some(edge) = param.get(bound) {
                    let mut case = base.clone();
                    case.insert(param_id.to_string(), edge.clone());
                    cases.push(case);
                }
            }
        }
        for params in cases {
            let params = Value::Object(params);
            for (w, h) in [(5u32, 3u32), (1, 1)] {
                let out = render(id, tiny(w, h), &params).unwrap_or_else(|err| {
                    panic!("effect {id} failed on {w}x{h} with params {params}: {err}")
                });
                assert!(out.width() > 0 && out.height() > 0, "{id} on {w}x{h}");
            }
        }
    }
}

#[test]
fn integer_slider_values() {
    for (effect, low, high) in [
        ("saturation", json!({"percentage": -100.0}), json!({"percentage": 100.0})),
        ("pixelate", json!({"scale": 2.0}), json!({"scale": 18.0})),
        ("chromatic", json!({"offset": 1.0}), json!({"offset": 30.0})),
        ("posterize", json!({"levels": 2.0}), json!({"levels": 24.0})),
        ("grain", json!({"amount": 1.0}), json!({"amount": 90.0})),
        ("kaleidoscope", json!({"segments": 2.0}), json!({"segments": 11.0})),
        ("kuwahara", json!({"radius": 1.0}), json!({"radius": 9.0})),
    ] {
        let low = render(effect, detailed(64, 48), &low).unwrap().into_rgba8().into_raw();
        let high = render(effect, detailed(64, 48), &high).unwrap().into_rgba8().into_raw();
        assert_ne!(low, high, "{effect}");
    }
}

#[test]
fn contrast_sigmoid() {
    for factor in [-100.0, 0.0, 100.0] {
        let params = json!({ "mode": "sigmoid", "factor": factor });
        let out = apply_contrast(tiny(5, 3), &params);
        assert_eq!((out.width(), out.height()), (5, 3));
    }
}

fn gray_ramp() -> DynamicImage {
    DynamicImage::ImageRgba8(RgbaImage::from_fn(256, 1, |x, _| {
        let v = x as u8;
        Rgba([v, v, v, 255])
    }))
}

#[test]
fn gamma_lut_monotonic() {
    for gamma in [0.5, 1.0, 2.2] {
        let out = apply_gamma(gray_ramp(), &json!({ "gamma": gamma })).into_rgba8();
        assert_eq!(out.get_pixel(0, 0)[0], 0);
        assert_eq!(out.get_pixel(255, 0)[0], 255);
        for x in 1..256u32 {
            let prev = out.get_pixel(x - 1, 0)[0];
            let cur = out.get_pixel(x, 0)[0];
            assert!(cur >= prev);
        }
    }
}

#[test]
fn contrast_zero_identity() {
    let out = apply_contrast(gray_ramp(), &json!({ "mode": "normal", "factor": 0.0 })).into_rgba8();
    for x in 0..256u32 {
        let got = out.get_pixel(x, 0)[0];
        assert!(got.abs_diff(x as u8) <= 1);
    }
}

#[test]
fn pixelate_dims() {
    let src = tiny(20, 20);
    let out = apply_pixelate(&src, &json!({ "scale": 5 }));
    assert_eq!((out.width(), out.height()), (20, 20));
    assert_eq!((src.width(), src.height()), (20, 20));
}

#[test]
fn border_frame() {
    let params = json!({ "color": "#ff0000", "thickness": 2, "radius": 0 });
    let out = apply_border(tiny(5, 3), &params);
    assert_eq!((out.width(), out.height()), (9, 7));
    let rgba = out.into_rgba8();
    assert_eq!(rgba.get_pixel(0, 0), &Rgba([255, 0, 0, 255]));
}

#[test]
fn round_corners() {
    let out = apply_round(tiny(20, 20), &json!({ "radius": 6 })).into_rgba8();
    assert_eq!(out.get_pixel(0, 0)[3], 0);
    assert_eq!(out.get_pixel(10, 10)[3], 255);

    let huge = apply_round(tiny(20, 20), &json!({ "radius": 1000 })).into_rgba8();
    assert_eq!(huge.get_pixel(0, 0)[3], 0);
}

#[test]
fn parse_hex_digits() {
    assert_eq!(crate::imgutil::parse_hex_argb("#1a2b3c"), Some((0x1a, 0x2b, 0x3c)));
    assert_eq!(crate::imgutil::parse_hex_argb("1a2b3c"), Some((0x1a, 0x2b, 0x3c)));
    assert_eq!(crate::imgutil::parse_hex_argb("#801a2b3c"), Some((0x1a, 0x2b, 0x3c)));
    assert_eq!(crate::imgutil::parse_hex_argb("#fff"), None);
    assert_eq!(crate::imgutil::parse_hex_argb("#zzzzzz"), None);
}

struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("skwd-effects-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn render_file_resize() {
    let dir = TempDir::new("render");
    for ext in ["png", "webp"] {
        let input = dir.0.join(format!("in.{ext}"));
        tiny(24, 18).save(&input).unwrap();
        let written = render_to_file("invert", &input, &json!({}), &dir.0.join("out"), 8, false)
            .await
            .unwrap();
        assert_eq!(written.extension().unwrap(), ext);
        let out = ImageReader::open(&written).unwrap().with_guessed_format().unwrap();
        let decoded = out.decode().unwrap();
        assert!(decoded.width() <= 8 && decoded.height() <= 8);
    }
}

#[tokio::test]
async fn render_file_dispatch() {
    let dir = TempDir::new("dispatch");
    let input = dir.0.join("in.png");
    tiny(24, 18).save(&input).unwrap();
    let output = dir.0.join("out.png");
    render_to_file("mirror", &input, &json!({}), &output, 0, false).await.unwrap();
    let decoded = ImageReader::open(&output).unwrap().decode().unwrap();
    assert_eq!((decoded.width(), decoded.height()), (24, 18));

    let bogus = dir.0.join("bogus.png");
    let err = render_to_file("nope", &input, &json!({}), &bogus, 0, false).await.unwrap_err();
    assert!(err.to_string().contains("unknown effect"));
    assert!(!bogus.exists());
}

#[tokio::test]
async fn render_effect_stack() {
    let dir = TempDir::new("stack");
    let input = dir.0.join("in.png");
    detailed(24, 18).save(&input).unwrap();
    let output = dir.0.join("out.png");
    let effects =
        [json!({"effect": "invert", "params": {}}), json!({"effect": "invert", "params": {}})];
    render_effects_to_file(&effects, &input, &output, 0, false).await.unwrap();
    let source = ImageReader::open(&input).unwrap().decode().unwrap().into_rgba8();
    let rendered = ImageReader::open(&output).unwrap().decode().unwrap().into_rgba8();
    assert_eq!(source, rendered);
}

#[test]
fn list_contract() {
    let listed = list();
    let arr = listed.as_array().unwrap();
    assert!(arr.len() >= 10);
    let names: Vec<&str> =
        arr.iter().filter_map(|entry| entry.get("id").and_then(|id| id.as_str())).collect();
    assert!(names.contains(&"invert"));
    assert!(names.contains(&"grayscale"));

    let invert = arr.iter().find(|entry| entry["id"] == "invert").unwrap();
    assert!(invert["label"].is_string());
    assert!(invert["description"].is_string());
    assert!(invert["params"].is_array());

    for effect in arr {
        for param in effect["params"].as_array().unwrap() {
            assert!(param["id"].is_string());
            assert!(param["label"].is_string());
            assert!(param["type"].is_string());
        }
    }
}

#[test]
fn effect_categories() {
    let listed = list();
    for effect in listed.as_array().unwrap() {
        let category = effect["category"].as_str().unwrap_or("");
        assert!(!category.is_empty());
    }
    let theme = listed.as_array().unwrap().iter().find(|entry| entry["id"] == "theme").unwrap();
    assert_eq!(theme["category"], "Colour");
}

#[test]
fn output_format_follows_input() {
    for (input, alpha, want) in [
        ("a.webp", false, "webp"),
        ("a.webp", true, "webp"),
        ("a.jpg", false, "jpg"),
        ("a.jpeg", false, "jpg"),
        ("a.jpg", true, "png"),
        ("a.png", false, "png"),
        ("a.bmp", false, "png"),
        ("a", false, "png"),
    ] {
        assert_eq!(target_ext(Path::new(input), alpha), want, "{input} alpha={alpha}");
    }
}

#[test]
fn alpha_detection() {
    assert!(!has_alpha(&tiny(4, 4)));
    let mut buf = tiny(4, 4).to_rgba8();
    buf.get_pixel_mut(0, 0).0[3] = 0;
    assert!(has_alpha(&DynamicImage::ImageRgba8(buf)));
}

#[tokio::test]
async fn render_reports_path() {
    let dir = TempDir::new("reported");
    let src = dir.0.join("src.png");
    tiny(8, 8).save(&src).unwrap();
    let written = render_to_file("invert", &src, &json!({}), &dir.0.join("out"), 0, false)
        .await
        .expect("render");
    assert_eq!(written.extension().unwrap(), "png");
    assert!(written.is_file());
}

fn riff(chunk: [u8; 4], tail: &[u8]) -> Vec<u8> {
    let mut buf = b"RIFF\0\0\0\0WEBP".to_vec();
    buf.extend_from_slice(&chunk);
    buf.extend_from_slice(tail);
    buf
}

#[test]
fn webp_lossless_detection() {
    assert!(webp_is_lossless(&riff(*b"VP8L", &[0; 32])));
    assert!(!webp_is_lossless(&riff(*b"VP8 ", &[0; 32])));

    let mut ext_lossless = riff(*b"VP8X", &[0; 8]);
    ext_lossless.extend_from_slice(b"VP8L");
    ext_lossless.extend_from_slice(&[0; 8]);
    assert!(webp_is_lossless(&ext_lossless));

    let mut ext_lossy = riff(*b"VP8X", &[0; 8]);
    ext_lossy.extend_from_slice(b"VP8 ");
    ext_lossy.extend_from_slice(&[0; 8]);
    assert!(!webp_is_lossless(&ext_lossy));

    for junk in [&b""[..], &b"RIFF"[..], &b"not a webp file at all"[..]] {
        assert!(!webp_is_lossless(junk), "{junk:?}");
    }
}
