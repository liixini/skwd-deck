use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::Value;

use crate::config::Config;

pub use skwd_palette::Rgb;
use skwd_palette::{from_hsl, parse_hex as parse, to_hsl};

pub fn material_map(palette: &Value, dark: bool) -> HashMap<&'static str, Rgb> {
    let get = |key: &str, dflt: &str| {
        palette
            .get(key)
            .and_then(Value::as_str)
            .and_then(parse)
            .unwrap_or_else(|| parse(dflt).unwrap_or(Rgb(128, 128, 128)))
    };
    let primary = get("primary", "#88c0d0");
    let on_primary = get("primaryText", "#1a1a1a");
    let tertiary = get("tertiary", primary.hex().as_str());
    let surface = get("surface", "#2e3440");
    let on_surface = get("surfaceText", "#f2f2f2");
    let surface_variant = get("surfaceVariant", surface.hex().as_str());
    let surface_container = get("surfaceContainer", surface.hex().as_str());
    let background = get("background", surface.hex().as_str());
    let outline = get("outline", "#808080");

    let (error, on_error, error_container, on_error_container) = if dark {
        (Rgb(0xf2, 0xb8, 0xb5), Rgb(0x60, 0x14, 0x10), Rgb(0x8c, 0x1d, 0x18), Rgb(0xf9, 0xde, 0xdc))
    } else {
        (Rgb(0xb3, 0x26, 0x1e), Rgb(0xff, 0xff, 0xff), Rgb(0xf9, 0xde, 0xdc), Rgb(0x41, 0x0e, 0x0b))
    };

    let black = Rgb(0, 0, 0);
    let mut map: HashMap<&'static str, Rgb> = HashMap::new();
    map.insert("primary", primary);
    map.insert("on_primary", on_primary);
    map.insert("primary_container", primary.lerp(surface, 0.65));
    map.insert("on_primary_container", on_surface);
    map.insert("secondary", primary.lerp(outline, 0.45));
    map.insert("on_secondary", on_primary);
    map.insert("secondary_container", primary.lerp(surface, 0.72));
    map.insert("on_secondary_container", on_surface);
    map.insert("tertiary", tertiary);
    map.insert("on_tertiary", on_primary);
    map.insert("tertiary_container", tertiary.lerp(surface, 0.72));
    map.insert("on_tertiary_container", on_surface);
    map.insert("surface", surface);
    map.insert("on_surface", on_surface);
    map.insert("surface_variant", surface_variant);
    map.insert("on_surface_variant", on_surface.lerp(surface, 0.25));
    map.insert("surface_container", surface_container);
    map.insert("surface_container_lowest", background);
    map.insert("surface_container_low", surface_container.lerp(background, 0.5));
    map.insert("surface_container_high", surface_container.lerp(on_surface, 0.08));
    map.insert("surface_container_highest", surface_container);
    map.insert("surface_dim", background.lerp(black, 0.15));
    map.insert("surface_bright", surface.lerp(on_surface, 0.12));
    map.insert("background", background);
    map.insert("on_background", on_surface);
    map.insert("outline", outline);
    map.insert("outline_variant", outline.lerp(surface, 0.55));
    map.insert("inverse_surface", on_surface);
    map.insert("inverse_on_surface", surface);
    map.insert("inverse_primary", primary.lerp(background, 0.4));
    map.insert("error", error);
    map.insert("on_error", on_error);
    map.insert("error_container", error_container);
    map.insert("on_error_container", on_error_container);
    map.insert("shadow", black);
    map.insert("scrim", black);
    map.insert("source_color", primary);
    let (_, sat, light) = to_hsl(primary);
    let sat = sat.max(0.35);
    let base = if dark { light.clamp(0.6, 0.78) } else { light.clamp(0.3, 0.45) };
    let lift = if dark { (base + 0.08).min(0.9) } else { (base - 0.08).max(0.2) };
    for (slot, bright, hue) in [
        ("ansi_red", "ansi_red_bright", 10.0),
        ("ansi_yellow", "ansi_yellow_bright", 65.0),
        ("ansi_green", "ansi_green_bright", 145.0),
        ("ansi_cyan", "ansi_cyan_bright", 185.0),
        ("ansi_blue", "ansi_blue_bright", 230.0),
        ("ansi_magenta", "ansi_magenta_bright", 315.0),
    ] {
        map.insert(slot, from_hsl(hue, sat, base));
        map.insert(bright, from_hsl(hue, sat, lift));
    }
    map
}

fn format_token(col: Rgb, fmt: &str) -> Option<String> {
    match fmt {
        "hex" => Some(col.hex()),
        "hex_stripped" => Some(col.hex()[1..].to_string()),
        "red" => Some(col.0.to_string()),
        "green" => Some(col.1.to_string()),
        "blue" => Some(col.2.to_string()),
        "rgb" => Some(format!("rgb({}, {}, {})", col.0, col.1, col.2)),
        "rgba" => Some(format!("rgba({}, {}, {}, 1.0)", col.0, col.1, col.2)),
        _ => None,
    }
}

fn expand(template: &str, resolve: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[start..]);
            return out;
        };
        if let Some(val) = resolve(after[..end].trim()) {
            out.push_str(&val);
        } else {
            out.push_str("{{");
            out.push_str(&after[..end]);
            out.push_str("}}");
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

fn split_placeholder(inner: &str) -> Option<(&str, &str, &str)> {
    let body = inner.strip_prefix("colors.")?;
    let mut parts = body.split('.');
    let token = parts.next()?;
    let scheme = parts.next()?;
    let fmt = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    Some((token, scheme, fmt))
}

pub fn render<S: std::hash::BuildHasher>(
    template: &str,
    map: &HashMap<&'static str, Rgb, S>,
) -> String {
    expand(template, |inner| {
        let (token, _scheme, fmt) = split_placeholder(inner)?;
        map.iter().find(|(key, _)| **key == token).and_then(|(_, col)| format_token(*col, fmt))
    })
}

pub fn render_doc(template: &str, doc: &serde_json::Value) -> String {
    expand(template, |inner| {
        if let Some(body) = inner.strip_prefix("base16.") {
            let mut parts = body.split('.');
            let slot = parts.next()?;
            let fmt = parts.next_back().unwrap_or("hex");
            let hex = doc.get("base16")?.get(slot)?.as_str()?;
            return format_token(parse(hex)?, fmt);
        }
        let (role, scheme, fmt) = split_placeholder(inner)?;
        let hex = crate::material::role(doc, role, scheme)?;
        format_token(parse(&hex)?, fmt)
    })
}

pub fn render_bridge(config: &Config, palette: &Value, dark: bool) -> Option<String> {
    let bridge = PathBuf::from(config.cache_dir()).join("colors.json");
    let template_dir = config.theme().templates_dir();
    let map = material_map(palette, dark);
    for integ in config.theme().integrations() {
        if integ.template.is_empty() || integ.output.is_empty() {
            continue;
        }
        let output = if integ.output.contains('/') {
            PathBuf::from(config.resolve(&integ.output))
        } else {
            PathBuf::from(config.cache_dir()).join(&integ.output)
        };
        if output != bridge {
            continue;
        }
        let input = if integ.template.contains('/') {
            PathBuf::from(config.resolve(&integ.template))
        } else {
            template_dir.join(&integ.template)
        };
        let text = std::fs::read_to_string(&input).ok()?;
        return Some(render(&text, &map));
    }
    None
}

pub fn integration_output(config: &Config, output: &str) -> PathBuf {
    if output.contains('/') {
        PathBuf::from(config.resolve(output))
    } else {
        PathBuf::from(config.cache_dir()).join(output)
    }
}

pub fn render_integrations(config: &Config, palette: &Value, dark: bool) {
    render_integrations_where(config, palette, dark, |_| true);
}

pub fn render_integrations_where(
    config: &Config,
    palette: &Value,
    dark: bool,
    keep: impl Fn(&crate::config::Integration) -> bool,
) -> usize {
    let map = material_map(palette, dark);
    let template_dir = config.theme().templates_dir();
    let mut written = 0usize;
    for integ in config.theme().integrations() {
        if integ.template.is_empty() || integ.output.is_empty() || !keep(&integ) {
            continue;
        }
        let input = if integ.template.contains('/') {
            PathBuf::from(config.resolve(&integ.template))
        } else {
            template_dir.join(&integ.template)
        };
        let Ok(text) = std::fs::read_to_string(&input) else {
            log::warn!("static templates: template not found: {}", input.display());
            continue;
        };
        let output = integration_output(config, &integ.output);
        if let Some(parent) = output.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::write(&output, render(&text, &map)) {
            Ok(()) => written += 1,
            Err(err) => log::warn!("static templates: write {} failed: {err}", output.display()),
        }
    }
    if written > 0 {
        log::info!("static templates: rendered {written} integration file(s)");
    }
    written
}

#[path = "tests.rs"]
mod tests;
