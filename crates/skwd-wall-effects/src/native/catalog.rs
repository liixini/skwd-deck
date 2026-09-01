use serde_json::{Value, json};

use skwd_palette::gowall as themes;

fn with_category(mut schema: Value, category: &str) -> Value {
    schema["category"] = Value::String(category.to_string());
    schema
}

pub(crate) fn list() -> Value {
    let mut effects = vec![
        with_category(theme_schema(), "Colour"),
        with_category(
            simple_schema("invert", "Invert", "Swap every colour for its opposite."),
            "Adjust",
        ),
        with_category(simple_schema("flip", "Flip", "Turn the image upside down."), "Transform"),
        with_category(simple_schema("mirror", "Mirror", "Swap left and right."), "Transform"),
        with_category(simple_schema("grayscale", "Grayscale", "Remove all colour."), "Adjust"),
        with_category(brightness_schema(), "Adjust"),
        with_category(contrast_schema(), "Adjust"),
        with_category(saturation_schema(), "Adjust"),
        with_category(gamma_schema(), "Adjust"),
        with_category(pixelate_schema(), "Stylize"),
        with_category(border_schema(), "Transform"),
        with_category(round_schema(), "Transform"),
    ];
    effects.extend(crate::registry::schemas());
    Value::Array(effects)
}

fn simple_schema(id: &str, label: &str, description: &str) -> Value {
    json!({ "id": id, "label": label, "description": description, "params": [] })
}

fn swatch_for(name: &str) -> Vec<String> {
    let Some(palette) = themes::lookup(name) else {
        return Vec::new();
    };
    if palette.is_empty() {
        return Vec::new();
    }
    let count = 6.min(palette.len());
    (0..count)
        .map(|slot| {
            let index = if count > 1 { slot * (palette.len() - 1) / (count - 1) } else { 0 };
            let (red, green, blue) = palette[index];
            format!("#{red:02x}{green:02x}{blue:02x}")
        })
        .collect()
}

fn theme_schema() -> Value {
    let options: Vec<Value> = themes::names()
        .into_iter()
        .map(|name| json!({ "mode": name, "label": name, "swatch": swatch_for(name) }))
        .collect();
    let default = themes::names().first().copied().unwrap_or("Catppuccin");
    json!({
        "id": "theme",
        "label": "Palette",
        "description": "Replace each colour with the closest one in a palette.",
        "params": [
            { "id": "theme", "label": "Palette", "type": "dropdown",
              "default": default, "options": options }
        ]
    })
}

fn brightness_schema() -> Value {
    json!({
        "id": "brightness",
        "label": "Brightness",
        "description": "Make the image lighter or darker. Move the slider right for lighter.",
        "params": [
            { "id": "factor", "label": "Brightness", "type": "number",
              "min": 0.1, "max": 10.0, "step": 0.05, "decimals": 2, "default": 1.1 }
        ]
    })
}

fn contrast_schema() -> Value {
    json!({
        "id": "contrast",
        "label": "Contrast",
        "description": "Pull light and dark areas further apart. Move the slider left to flatten them.",
        "params": [
            { "id": "mode", "label": "Mode", "type": "dropdown", "default": "normal",
              "options": [
                  { "mode": "normal",  "label": "Normal" },
                  { "mode": "sigmoid", "label": "Sigmoid" }
              ] },
            { "id": "factor", "label": "Contrast", "type": "number",
              "min": -100.0, "max": 100.0, "step": 1.0, "decimals": 1, "default": 25.0 }
        ]
    })
}

fn saturation_schema() -> Value {
    json!({
        "id": "saturation",
        "label": "Saturation",
        "description": "Make the colours stronger or weaker.",
        "params": [
            { "id": "percentage", "label": "Saturation", "type": "integer",
              "min": -100, "max": 100, "step": 1, "default": 25 }
        ]
    })
}

fn gamma_schema() -> Value {
    json!({
        "id": "gamma",
        "label": "Gamma",
        "description": "Change the midtones while mostly leaving black and white alone.",
        "params": [
            { "id": "gamma", "label": "Gamma", "type": "number",
              "min": 0.1, "max": 5.0, "step": 0.05, "decimals": 2, "default": 1.0 }
        ]
    })
}

fn pixelate_schema() -> Value {
    json!({
        "id": "pixelate",
        "label": "Pixelate",
        "description": "Group pixels into blocks. Move the slider right for larger blocks.",
        "params": [
            { "id": "scale", "label": "Block size", "type": "integer",
              "min": 2, "max": 100, "step": 1, "default": 15 }
        ]
    })
}

fn border_schema() -> Value {
    json!({
        "id": "border",
        "label": "Border",
        "description": "Put a solid colour around the image.",
        "params": [
            { "id": "color",     "label": "Colour",    "type": "color",   "default": "#1a1a1a" },
            { "id": "thickness", "label": "Thickness", "type": "integer",
              "min": 0, "max": 500, "step": 1, "default": 30 },
            { "id": "radius",    "label": "Radius",    "type": "integer",
              "min": 0, "max": 500, "step": 1, "default": 0 }
        ]
    })
}

fn round_schema() -> Value {
    json!({
        "id": "round",
        "label": "Round corners",
        "description": "Cut away the corners. Move the slider right for rounder corners.",
        "params": [
            { "id": "radius", "label": "Radius", "type": "integer",
              "min": 1, "max": 1000, "step": 1, "default": 60 }
        ]
    })
}
