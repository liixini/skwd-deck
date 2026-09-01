use serde_json::Value;

pub const STYLES: [&str; 4] = ["natural", "pastel", "muted", "vibrant"];
const TINTED: [&str; 7] = [
    "primary",
    "tertiary",
    "surface",
    "surfaceVariant",
    "surfaceContainer",
    "background",
    "outline",
];
const MIN_TEXT_CONTRAST: f32 = 4.5;

#[derive(Clone, Copy, PartialEq)]
enum Direction {
    Up,
    Down,
    Both,
}

struct Recipe {
    direction: Direction,
    sat_target: f32,
    sat_pull: f32,
    light_target: f32,
    light_pull: f32,
}

fn recipe(style: &str) -> Option<Recipe> {
    match style {
        "pastel" => Some(Recipe {
            direction: Direction::Both,
            sat_target: 0.45,
            sat_pull: 0.80,
            light_target: 0.86,
            light_pull: 0.55,
        }),
        "muted" => Some(Recipe {
            direction: Direction::Down,
            sat_target: 0.14,
            sat_pull: 0.85,
            light_target: 0.0,
            light_pull: 0.0,
        }),
        "vibrant" => Some(Recipe {
            direction: Direction::Up,
            sat_target: 0.85,
            sat_pull: 0.60,
            light_target: 0.0,
            light_pull: 0.0,
        }),
        _ => None,
    }
}

fn toward(value: f32, target: f32, pull: f32) -> f32 {
    (value + (target - value) * pull).clamp(0.0, 1.0)
}

fn restyle_one(hex: &str, recipe: &Recipe, dark: bool, is_text_backdrop: bool) -> Option<String> {
    let col = skwd_palette::parse_hex(hex)?;
    let (hue, sat, light) = skwd_palette::to_hsl(col);
    let pulled = toward(sat, recipe.sat_target, recipe.sat_pull);
    let sat = match recipe.direction {
        Direction::Down => pulled.min(sat),
        Direction::Up => pulled.max(sat),
        Direction::Both => pulled,
    };
    let light = if dark || recipe.light_pull <= 0.0 || !is_text_backdrop {
        light
    } else {
        toward(light, recipe.light_target, recipe.light_pull).min(0.95)
    };
    Some(skwd_palette::from_hsl(hue, sat, light).hex())
}

fn lightness_of(hex: &str) -> Option<f32> {
    skwd_palette::parse_hex(hex).map(|col| skwd_palette::to_hsl(col).2)
}

fn shift_light(hex: &str, delta: f32) -> String {
    let Some(col) = skwd_palette::parse_hex(hex) else {
        return hex.to_string();
    };
    let (hue, sat, light) = skwd_palette::to_hsl(col);
    skwd_palette::from_hsl(hue, sat, (light + delta).clamp(0.02, 0.98)).hex()
}

pub fn is_dark_palette(palette: &Value) -> bool {
    let get =
        |key: &str| palette.get(key).and_then(Value::as_str).and_then(skwd_palette::parse_hex);
    let (Some(surface), Some(text)) = (get("surface"), get("surfaceText")) else {
        return true;
    };
    skwd_palette::to_hsl(surface).2 < skwd_palette::to_hsl(text).2
}

pub fn restyle(palette: &Value, style: &str) -> Value {
    let Some(recipe) = recipe(style) else {
        return palette.clone();
    };
    let Some(obj) = palette.as_object() else {
        return palette.clone();
    };
    let dark = is_dark_palette(palette);
    let mut out = obj.clone();
    let shift = obj
        .get("surface")
        .and_then(Value::as_str)
        .and_then(|hex| {
            let before = lightness_of(hex)?;
            let after = lightness_of(&restyle_one(hex, &recipe, dark, true)?)?;
            Some(after - before)
        })
        .unwrap_or(0.0);
    for role in TINTED {
        let backdrop =
            matches!(role, "surface" | "surfaceVariant" | "surfaceContainer" | "background");
        if let Some(hex) = obj.get(role).and_then(Value::as_str)
            && let Some(next) = restyle_one(hex, &recipe, dark, false)
        {
            let next = if backdrop { shift_light(&next, shift) } else { next };
            out.insert(role.to_string(), Value::String(next));
        }
    }
    let styled = Value::Object(out);
    if readable(&styled) { styled } else { palette.clone() }
}

fn readable(palette: &Value) -> bool {
    let get =
        |key: &str| palette.get(key).and_then(Value::as_str).and_then(skwd_palette::parse_hex);
    let (Some(surface), Some(text)) = (get("surface"), get("surfaceText")) else {
        return true;
    };
    skwd_palette::semantic::contrast(text, surface) >= MIN_TEXT_CONTRAST
}

#[path = "tests.rs"]
mod tests;
