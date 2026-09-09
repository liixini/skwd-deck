use serde_json::{Map, Value, json};

pub const SOURCE_ROLE: &str = "source_color";

fn hex_of(val: &Value) -> Option<String> {
    let chan = |key: &str| val.get(key).and_then(Value::as_u64);
    Some(format!("#{:02x}{:02x}{:02x}", chan("red")?, chan("green")?, chan("blue")?))
}

fn variants(dark_hex: &str, light_hex: &str, dark: bool) -> Value {
    let default = if dark { dark_hex } else { light_hex };
    json!({
        "dark": {"color": dark_hex},
        "light": {"color": light_hex},
        "default": {"color": default},
    })
}

pub fn parse_seed(seed: &str) -> Option<String> {
    let digits = seed.trim().trim_start_matches('#');
    if digits.len() != 6 || !digits.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("#{}", digits.to_ascii_lowercase()))
}

pub const SCHEMES: [&str; 9] = [
    "tonal-spot",
    "vibrant",
    "expressive",
    "neutral",
    "monochrome",
    "fidelity",
    "content",
    "rainbow",
    "fruit-salad",
];

fn variant_of(name: &str) -> material_colors::dynamic_color::Variant {
    use material_colors::dynamic_color::Variant;
    match name {
        "vibrant" => Variant::Vibrant,
        "expressive" => Variant::Expressive,
        "neutral" => Variant::Neutral,
        "monochrome" => Variant::Monochrome,
        "fidelity" => Variant::Fidelity,
        "content" => Variant::Content,
        "rainbow" => Variant::Rainbow,
        "fruit-salad" => Variant::FruitSalad,
        _ => Variant::TonalSpot,
    }
}

pub fn document(seed: &str, dark: bool) -> Option<Value> {
    document_with(seed, dark, "tonal-spot")
}

pub const BASE16_KEYS: [&str; 16] = [
    "base00", "base01", "base02", "base03", "base04", "base05", "base06", "base07", "base08",
    "base09", "base0A", "base0B", "base0C", "base0D", "base0E", "base0F",
];

fn accent_at(seed: &str, hue: f32, dark: bool) -> String {
    let Some(col) = crate::parse_hex(seed) else {
        return seed.to_string();
    };
    let (_, sat, _) = crate::to_hsl(col);
    let sat = sat.clamp(0.35, 0.80);
    let light = if dark { 0.68 } else { 0.42 };
    crate::from_hsl(hue, sat, light).hex()
}

fn base16_of(
    colors: &Map<String, Value>,
    seed: &str,
    dark: bool,
    scheme: &str,
) -> Map<String, Value> {
    let role = |name: &str| {
        colors
            .get(name)
            .and_then(|node| node.get(scheme))
            .and_then(|node| node.get("color"))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let pick = |names: &[&str]| {
        names.iter().find_map(|name| role(name)).unwrap_or_else(|| seed.to_string())
    };
    let hue = |deg: f32| accent_at(seed, deg, dark);
    let mut out = Map::new();
    let values = [
        pick(&["surface_container_lowest", "background", "surface"]),
        pick(&["surface_container_low", "surface_container", "surface"]),
        pick(&["surface_container", "surface_variant"]),
        pick(&["outline", "on_surface_variant"]),
        pick(&["on_surface_variant", "outline"]),
        pick(&["on_surface"]),
        pick(&["on_surface"]),
        pick(&["surface_bright", "on_surface"]),
        pick(&["error"]),
        hue(28.0),
        hue(52.0),
        hue(120.0),
        hue(190.0),
        pick(&["primary"]),
        pick(&["tertiary", "secondary"]),
        hue(14.0),
    ];
    for (key, val) in BASE16_KEYS.iter().zip(values) {
        out.insert((*key).to_string(), Value::String(val));
    }
    out
}

pub fn document_with(seed: &str, dark: bool, scheme: &str) -> Option<Value> {
    let seed = parse_seed(seed)?;
    let source: material_colors::color::Argb = seed.parse().ok()?;
    let theme = material_colors::theme::ThemeBuilder::with_source(source)
        .variant(variant_of(scheme))
        .build();
    let dark_val = serde_json::to_value(&theme.schemes.dark).ok()?;
    let light_val = serde_json::to_value(&theme.schemes.light).ok()?;
    let (dark_obj, light_obj) = (dark_val.as_object()?, light_val.as_object()?);

    let mut colors = Map::with_capacity(dark_obj.len() + 1);
    for (role, dark_col) in dark_obj {
        let Some(dark_hex) = hex_of(dark_col) else {
            continue;
        };
        let Some(light_hex) = light_obj.get(role).and_then(hex_of) else {
            continue;
        };
        colors.insert(role.clone(), variants(&dark_hex, &light_hex, dark));
    }
    if colors.is_empty() {
        return None;
    }
    colors.insert(SOURCE_ROLE.to_string(), variants(&seed, &seed, dark));

    let mode = if dark { "dark" } else { "light" };
    let base16 = base16_of(&colors, &seed, dark, mode);
    Some(json!({
        "colors": colors,
        "base16": base16,
        "mode": if dark { "dark" } else { "light" },
        "is_dark_mode": dark,
    }))
}

pub fn role(doc: &Value, name: &str, scheme: &str) -> Option<String> {
    doc.get("colors")?.get(name)?.get(scheme)?.get("color")?.as_str().map(str::to_string)
}

#[path = "tests.rs"]
mod tests;

pub const ROLE_KEYS: [&str; 50] = [
    "primary",
    "on_primary",
    "tertiary",
    "surface",
    "on_surface",
    "surface_variant",
    "surface_container",
    "background",
    "outline",
    "error",
    "error_container",
    "inverse_on_surface",
    "inverse_primary",
    "inverse_surface",
    "on_background",
    "on_error",
    "on_error_container",
    "on_primary_container",
    "on_primary_fixed",
    "on_primary_fixed_variant",
    "on_secondary",
    "on_secondary_container",
    "on_secondary_fixed",
    "on_secondary_fixed_variant",
    "on_surface_variant",
    "on_tertiary",
    "on_tertiary_container",
    "on_tertiary_fixed",
    "on_tertiary_fixed_variant",
    "outline_variant",
    "primary_container",
    "primary_fixed",
    "primary_fixed_dim",
    "scrim",
    "secondary",
    "secondary_container",
    "secondary_fixed",
    "secondary_fixed_dim",
    "shadow",
    "surface_bright",
    "surface_container_high",
    "surface_container_highest",
    "surface_container_low",
    "surface_container_lowest",
    "surface_dim",
    "surface_tint",
    "tertiary_container",
    "tertiary_fixed",
    "tertiary_fixed_dim",
    "source_color",
];

pub const UI_KEYS: [&str; 9] = [
    "primary",
    "primaryText",
    "tertiary",
    "surface",
    "surfaceText",
    "surfaceVariant",
    "surfaceContainer",
    "background",
    "outline",
];

pub fn colors(doc: &Value, dark: bool) -> [String; 50] {
    std::array::from_fn(|index| {
        role(doc, ROLE_KEYS[index], if dark { "dark" } else { "light" })
            .and_then(|hex| parse_seed(&hex))
            .unwrap_or_else(|| "#808080".to_string())
    })
}

pub fn generate_colors(seed: &str) -> Option<([String; 50], [String; 50])> {
    let doc = document(seed, true)?;
    Some((colors(&doc, true), colors(&doc, false)))
}

pub fn from_colors(dark_colors: &[String; 50], light_colors: &[String; 50], dark: bool) -> Value {
    let mut colors = Map::new();
    for (index, key) in ROLE_KEYS.iter().enumerate() {
        colors
            .insert((*key).to_string(), variants(&dark_colors[index], &light_colors[index], dark));
    }
    let mut doc = json!({"colors": colors});
    select_mode(&mut doc, dark);
    doc
}

pub fn select_mode(doc: &mut Value, dark: bool) {
    let mode = if dark { "dark" } else { "light" };
    if let Some(colors) = doc.get_mut("colors").and_then(Value::as_object_mut) {
        for node in colors.values_mut() {
            if let Some(value) = node.get(mode).cloned() {
                node["default"] = value;
            }
        }
        let seed = colors
            .get(SOURCE_ROLE)
            .and_then(|node| node.get(mode))
            .and_then(|node| node.get("color"))
            .and_then(Value::as_str)
            .unwrap_or("#808080");
        let base16 = base16_of(colors, seed, dark, mode);
        doc["base16"] = Value::Object(base16);
    }
    doc["mode"] = json!(mode);
    doc["is_dark_mode"] = json!(dark);
}

pub fn from_palette(palette: &Value, dark: bool, scheme: &str) -> Option<Value> {
    if palette.get("_schemeVersion").and_then(Value::as_u64) == Some(1)
        && let Some(saved) = palette.get("_scheme")
        && ROLE_KEYS.iter().all(|key| {
            ["dark", "light"]
                .iter()
                .all(|mode| role(saved, key, mode).is_some_and(|hex| parse_seed(&hex).is_some()))
        })
    {
        let mut doc = saved.clone();
        select_mode(&mut doc, dark);
        return Some(doc);
    }
    let seed = palette.get("primary").and_then(Value::as_str).unwrap_or("#808080");
    let mut doc = document_with(seed, dark, scheme)?;
    if let Some(saved) =
        palette.get("_scheme").and_then(|value| value.get("colors")).and_then(Value::as_object)
    {
        for (key, node) in saved {
            for mode in ["dark", "light"] {
                if let Some(hex) = node
                    .get(mode)
                    .and_then(|value| value.get("color"))
                    .and_then(Value::as_str)
                    .and_then(parse_seed)
                {
                    doc["colors"][key][mode]["color"] = json!(hex);
                }
            }
        }
    }
    if palette.get("_schemeVersion").and_then(Value::as_u64) != Some(1) {
        for (index, key) in UI_KEYS.iter().enumerate() {
            if let Some(hex) = palette
                .get(*key)
                .or_else(|| palette.get(ROLE_KEYS[index]))
                .and_then(Value::as_str)
                .and_then(parse_seed)
            {
                doc["colors"][ROLE_KEYS[index]][if dark { "dark" } else { "light" }]["color"] =
                    json!(hex);
            }
        }
    }
    select_mode(&mut doc, dark);
    Some(doc)
}

pub fn ui_palette(doc: &Value) -> Option<Value> {
    let mut palette = Map::new();
    for (index, key) in UI_KEYS.iter().enumerate() {
        palette.insert((*key).to_string(), json!(role(doc, ROLE_KEYS[index], "default")?));
    }
    Some(Value::Object(palette))
}
