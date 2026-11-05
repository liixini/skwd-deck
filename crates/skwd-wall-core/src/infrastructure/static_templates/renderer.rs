use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::Value;

use crate::config::Config;

pub use skwd_palette::Rgb;
use skwd_palette::parse_hex as parse;

const BRIDGE_FILE: &str = "colors.json";
const TOKEN_OPEN: &str = "{{";
const TOKEN_CLOSE: &str = "}}";
const TERMINAL_KEYS: [(&str, &str, usize); 6] = [
    ("ansi_red", "ansi_red_bright", 0),
    ("ansi_green", "ansi_green_bright", 1),
    ("ansi_yellow", "ansi_yellow_bright", 2),
    ("ansi_blue", "ansi_blue_bright", 3),
    ("ansi_magenta", "ansi_magenta_bright", 4),
    ("ansi_cyan", "ansi_cyan_bright", 5),
];

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
    let terminal = skwd_palette::template_terminal(primary, dark);
    for (slot, bright, index) in TERMINAL_KEYS {
        let own = |key: &str| palette.get(key).and_then(Value::as_str).and_then(parse);
        map.insert(slot, own(slot).unwrap_or(terminal.normal[index]));
        map.insert(bright, own(bright).unwrap_or(terminal.bright[index]));
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
    while let Some(start) = rest.find(TOKEN_OPEN) {
        out.push_str(&rest[..start]);
        let after = &rest[start + TOKEN_OPEN.len()..];
        let mut literal = serde_json::Deserializer::from_str(after).into_iter::<String>();
        if let Some(Ok(text)) = literal.next()
            && let Some(tail) =
                after[literal.byte_offset()..].trim_start().strip_prefix(TOKEN_CLOSE)
        {
            out.push_str(&text);
            rest = tail;
            continue;
        }
        let Some(end) = after.find(TOKEN_CLOSE) else {
            out.push_str(&rest[start..]);
            return out;
        };
        if let Some(val) = resolve(after[..end].trim()) {
            out.push_str(&val);
        } else {
            out.push_str(TOKEN_OPEN);
            out.push_str(&after[..end]);
            out.push_str(TOKEN_CLOSE);
        }
        rest = &after[end + TOKEN_CLOSE.len()..];
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

pub(crate) fn render_palette(template: &str, palette: &Value, dark: bool) -> String {
    let Some(mut doc) = palette.get("_scheme").cloned() else {
        return render(template, &material_map(palette, dark));
    };
    crate::material::select_mode(&mut doc, dark);
    let rendered = render_doc(template, &doc);
    render(&rendered, &material_map(palette, dark))
}

pub fn render_bridge(config: &Config, palette: &Value, dark: bool) -> Option<String> {
    let bridge = PathBuf::from(config.cache_dir()).join(BRIDGE_FILE);
    let template_dir = config.theme().templates_dir();
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
        return Some(render_palette(&text, palette, dark));
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

pub fn is_bridge_output(config: &Config, output: &std::path::Path) -> bool {
    if std::fs::symlink_metadata(output).is_ok_and(|meta| meta.file_type().is_symlink())
        && output.canonicalize().is_err()
    {
        return true;
    }
    [
        PathBuf::from(config.cache_dir()).join(BRIDGE_FILE),
        config.theme().native_colors_path(),
        crate::theme::scheme_path(config),
    ]
    .iter()
    .any(|protected| crate::paths::same(output, protected))
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
        if !integ.profile.is_empty() && is_bridge_output(config, &output) {
            log::warn!("custom output {} targets the global palette; skipped", integ.name);
            continue;
        }
        if crate::theme::apps::protects_output(&output) {
            log::warn!("custom output {} targets a managed Waybar file; skipped", integ.name);
            continue;
        }
        let selected =
            match crate::theme::profiles::app_palette(config, &integ.profile, palette, dark) {
                Ok(selected) => selected,
                Err(err) => {
                    log::warn!("custom output {}: {err}", integ.name);
                    continue;
                }
            };
        if let Some(parent) = output.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::write(&output, render_palette(&text, &selected, dark)) {
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
