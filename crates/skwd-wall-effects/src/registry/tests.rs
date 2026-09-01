#![cfg(test)]

use super::*;
use image::RgbaImage;

fn default_params(schema: &Value) -> Value {
    let mut map = serde_json::Map::new();
    if let Some(params) = schema.get("params").and_then(|value| value.as_array()) {
        for param in params {
            if let (Some(id), Some(default)) =
                (param.get("id").and_then(|value| value.as_str()), param.get("default"))
            {
                map.insert(id.to_string(), default.clone());
            }
        }
    }
    Value::Object(map)
}

#[test]
#[allow(clippy::cast_possible_truncation)]
fn effects_render() {
    let img = DynamicImage::ImageRgba8(RgbaImage::from_fn(24, 18, |x, y| {
        image::Rgba([(x * 9) as u8, (y * 11) as u8, ((x + y) * 5) as u8, 255])
    }));
    let defs = all();
    assert!(defs.len() >= 20, "got {}", defs.len());
    for def in defs {
        let schema = (def.schema)();
        assert_eq!(schema.get("id").and_then(|value| value.as_str()), Some(def.id));
        let params = default_params(&schema);
        let out = (def.render)(img.clone(), &params)
            .unwrap_or_else(|err| panic!("effect {} failed: {err}", def.id));
        assert!(out.width() > 0 && out.height() > 0);
    }
}

#[test]
fn ids_unique() {
    let mut ids: Vec<&str> = all().iter().map(|effect| effect.id).collect();
    ids.sort_unstable();
    let before = ids.len();
    ids.dedup();
    assert_eq!(before, ids.len());
}
