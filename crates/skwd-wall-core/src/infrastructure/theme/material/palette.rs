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
    let Some(col) = skwd_palette::parse_hex(seed) else {
        return seed.to_string();
    };
    let (_, sat, _) = skwd_palette::to_hsl(col);
    let sat = sat.clamp(0.35, 0.80);
    let light = if dark { 0.68 } else { 0.42 };
    skwd_palette::from_hsl(hue, sat, light).hex()
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
