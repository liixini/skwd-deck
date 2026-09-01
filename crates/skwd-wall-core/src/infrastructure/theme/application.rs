use crate::lock;
use std::collections::HashMap;
use std::fmt::Write;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

use crate::config::Config;
use crate::matugen::{run_reloads, run_sh, shell_quote};

const WALLUST_MIN_COLORS: usize = 16;

use super::availability::resolve_backend;
#[cfg(test)]
use super::availability::{
    ALL_BACKENDS, available_backends, backend_available, effective_backend, probe_cached,
};

pub fn wallust_command(config: &Config, image: &str, dark: bool) -> String {
    let mut cmd = String::from("wallust run");
    if let Some(pal) = config.theme().wallust_palette() {
        let _ = write!(cmd, " -p {}", shell_quote(&pal));
    } else if !dark {
        cmd.push_str(" -p light16");
    }
    if let Some(cs) = config.theme().wallust_colorspace() {
        let _ = write!(cmd, " --colorspace {}", shell_quote(&cs));
    }
    cmd.push(' ');
    cmd.push_str(&shell_quote(image));
    cmd
}

pub fn iris_args(image: &str, dark: bool, json_only: bool) -> Vec<String> {
    let mut args = vec![image.to_string(), "--dark".to_string(), u8::from(dark).to_string()];
    if json_only {
        args.push("--json-only".to_string());
    }
    args
}

pub fn iris_pick(val: &serde_json::Value) -> Vec<String> {
    let get = |key: &str| val.get(key).and_then(serde_json::Value::as_str).map(String::from);
    let (Some(accent), Some(bg), Some(surface)) = (get("accent"), get("bg"), get("surface")) else {
        return Vec::new();
    };
    let dim = get("dim").unwrap_or_else(|| surface.clone());
    let tertiary = get("green").or_else(|| get("yellow")).unwrap_or_else(|| accent.clone());
    vec![accent, tertiary, surface.clone(), surface, bg, dim]
}

fn iris_colors_from_cache() -> Vec<String> {
    let path = xdg_cache_home().join("iris").join("colors.json");
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .map(|val| iris_pick(&val))
        .unwrap_or_default()
}

fn iris_preview(image: &str, dark: bool) -> Vec<String> {
    let mut cmd = Command::new("iris");
    cmd.args(iris_args(image, dark, true));
    run_capture(&mut cmd)
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .map(|val| iris_pick(&val))
        .unwrap_or_default()
}

fn skwd_iris_run(image: &str, dark: bool, auto: bool) -> Option<serde_json::Value> {
    let bin = crate::paths::sibling_bin("skwd-wall-scan");
    let mut cmd = Command::new(bin);
    cmd.arg("--semantic").arg(image);
    if auto {
        cmd.arg("--auto");
    } else if !dark {
        cmd.arg("--light");
    }
    run_capture(&mut cmd).and_then(|text| serde_json::from_str(&text).ok())
}

fn hex_rgb(hex: &str) -> Option<(f32, f32, f32)> {
    let digits = hex.trim().trim_start_matches('#');
    let chan = |idx: usize| u8::from_str_radix(digits.get(idx..idx + 2)?, 16).ok();
    Some((f32::from(chan(0)?), f32::from(chan(2)?), f32::from(chan(4)?)))
}

pub fn semantic_palette(val: &serde_json::Value) -> Option<(serde_json::Value, String)> {
    let get = |key: &str| val.get(key).and_then(serde_json::Value::as_str).map(String::from);
    let rgb =
        |hex: &str| hex_rgb(hex).map(|(r, g, b)| skwd_palette::Rgb(r as u8, g as u8, b as u8));
    let (bg, surface, fg, dim, accent) =
        (get("bg")?, get("surface")?, get("fg")?, get("dim")?, get("accent")?);
    let sem = skwd_palette::semantic::Semantic {
        bg: rgb(&bg)?,
        surface: rgb(&surface)?,
        fg: rgb(&fg)?,
        dim: rgb(&dim)?,
        accent: rgb(&accent)?,
    };
    let mut palette = skwd_palette::semantic::ui_palette(&sem);
    if let Some(green) = get("green").filter(|hex| hex != &accent)
        && let Some(obj) = palette.as_object_mut()
    {
        obj.insert("tertiary".to_string(), serde_json::Value::String(green));
    }
    Some((palette, accent))
}

fn ansi16_run(image: &str, dark: bool, variant: &str) -> Option<Vec<String>> {
    let bin = crate::paths::sibling_bin("skwd-wall-scan");
    let mut cmd = Command::new(bin);
    cmd.arg("--ansi16").arg(image).arg("--variant").arg(variant);
    if !dark {
        cmd.arg("--light");
    }
    let val: serde_json::Value =
        run_capture(&mut cmd).and_then(|text| serde_json::from_str(&text).ok())?;
    let cols: Vec<String> = val
        .get("colors")?
        .as_array()?
        .iter()
        .filter_map(|col| col.as_str().map(str::to_string))
        .collect();
    (cols.len() == 16).then_some(cols)
}

pub fn ansi_palette(cols: &[String]) -> Option<serde_json::Value> {
    if cols.len() != 16 {
        return None;
    }
    Some(serde_json::json!({
        "primary": cols[4],
        "primaryText": cols[0],
        "tertiary": cols[2],
        "surface": cols[0],
        "surfaceText": cols[7],
        "surfaceVariant": cols[8],
        "surfaceContainer": cols[8],
        "background": cols[0],
        "outline": cols[8],
    }))
}

fn ansi_preview(image: &str, dark: bool) -> Vec<String> {
    ansi16_run(image, dark, "dark16")
        .and_then(|cols| ansi_palette(&cols))
        .map(|palette| swatch_from_palette(&palette))
        .unwrap_or_default()
}

fn skwd_iris_preview(image: &str, dark: bool) -> Vec<String> {
    skwd_iris_run(image, dark, false)
        .and_then(|val| semantic_palette(&val))
        .map(|(palette, _)| swatch_from_palette(&palette))
        .unwrap_or_default()
}

pub fn pywal_command(config: &Config, image: &str, dark: bool) -> String {
    let light = if dark { "" } else { " -l" };
    let sat = config
        .theme()
        .pywal_saturate()
        .map_or_else(String::new, |sat| format!(" --saturate {}", shell_quote(&sat)));
    format!("wal -q -n{sat} -i {}{light}", shell_quote(image))
}

fn hex_luma(hex: &str) -> f32 {
    let digits = hex.trim_start_matches('#');
    let chan = |idx: usize| {
        digits
            .get(idx..idx + 2)
            .and_then(|pair| u8::from_str_radix(pair, 16).ok())
            .map_or(0.0, |val| f32::from(val) / 255.0)
    };
    0.2126 * chan(0) + 0.7152 * chan(2) + 0.0722 * chan(4)
}

pub fn picker_palette_json(cols: &[String]) -> Option<serde_json::Value> {
    if cols.len() < 6 {
        return None;
    }
    let text = |bg: &str| if hex_luma(bg) > 0.5 { "#1a1a1a" } else { "#f2f2f2" };
    Some(serde_json::json!({
        "primary": cols[0],
        "tertiary": cols[1],
        "surfaceVariant": cols[2],
        "surfaceContainer": cols[3],
        "surface": cols[4],
        "outline": cols[5],
        "background": cols[4],
        "surfaceText": text(&cols[4]),
        "primaryText": text(&cols[0]),
    }))
}

pub const SWATCH_KEYS: [&str; 6] =
    ["primary", "tertiary", "surfaceVariant", "surfaceContainer", "surface", "outline"];

pub fn swatch_from_palette(val: &serde_json::Value) -> Vec<String> {
    SWATCH_KEYS
        .iter()
        .filter_map(|key| val.get(*key).and_then(serde_json::Value::as_str).map(String::from))
        .collect()
}

fn native_swatch_from_disk(config: &Config) -> Vec<String> {
    let path = config.theme().native_colors_path();
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return Vec::new();
    };
    swatch_from_palette(&val)
}

pub fn scheme_path(config: &Config) -> std::path::PathBuf {
    std::path::PathBuf::from(config.cache_dir()).join("scheme.json")
}

fn scheme_doc(config: &Config, seed: &str, dark: bool) -> Option<serde_json::Value> {
    let doc = crate::material::document_with(seed, dark, &config.theme().scheme())?;
    let path = scheme_path(config);
    if let Err(err) = crate::paths::atomic_write(&path, doc.to_string().as_bytes()) {
        log::warn!("scheme: write {} failed: {err}", path.display());
        return None;
    }
    crate::theme_provider::publish(config, &doc);
    Some(doc)
}

pub fn write_scheme(config: &Config, seed: &str, dark: bool) -> bool {
    if scheme_doc(config, seed, dark).is_some() {
        return true;
    }
    log::warn!("scheme: {seed:?} did not yield a standardised scheme");
    false
}

pub fn palette_from_scheme(doc: &serde_json::Value) -> Option<serde_json::Value> {
    let role = |name: &str| crate::material::role(doc, name, "default");
    let surface = role("surface")?;
    let recessed =
        ["surface_dim", "surface_container_lowest", "surface_container_low", "background"]
            .iter()
            .filter_map(|name| role(name))
            .find(|col| *col != surface)?;
    Some(serde_json::json!({
        "primary": role("primary")?,
        "primaryText": role("on_primary")?,
        "tertiary": role("tertiary")?,
        "surface": surface,
        "surfaceText": role("on_surface")?,
        "surfaceVariant": role("surface_container")
            .or_else(|| role("surface_variant"))?,
        "surfaceContainer": role("surface_container_highest")
            .or_else(|| role("surface_container"))?,
        "background": recessed,
        "outline": role("outline")?,
    }))
}

pub fn publish_scheme(config: &Config, seed: &str, dark: bool) -> bool {
    let Some(doc) = scheme_doc(config, seed, dark) else {
        log::warn!("scheme: {seed:?} is not a usable seed colour, keeping the previous colours");
        return false;
    };
    let Some(palette) = palette_from_scheme(&doc) else {
        log::warn!("scheme: document is missing core roles, picker palette not updated");
        return false;
    };
    let styled = write_picker_palette_value(config, &palette);
    crate::static_templates::render_integrations(config, &styled, dark);
    true
}

fn publish_colors(config: &Config, cols: &[String], dark: bool, image: &str) {
    if let Some(seed) = cols.first()
        && publish_scheme(config, seed, dark)
    {
        return;
    }
    if cols.is_empty() {
        write_picker_palette(config, image);
    } else {
        write_picker_palette_cols(config, cols);
    }
}

pub(crate) fn write_picker_palette_value(
    config: &Config,
    json: &serde_json::Value,
) -> serde_json::Value {
    let styled = crate::style::restyle(json, &config.theme().style());
    let path = std::path::PathBuf::from(config.cache_dir()).join("colors.json");
    if let Err(err) = crate::paths::atomic_write(&path, styled.to_string().as_bytes()) {
        log::warn!("picker palette bridge: write {} failed: {err}", path.display());
    }
    styled
}

pub(crate) fn write_picker_palette_cols(config: &Config, cols: &[String]) {
    let Some(json) = picker_palette_json(cols) else {
        log::warn!("picker palette bridge: no colours extracted");
        return;
    };
    write_picker_palette_value(config, &json);
}

fn write_picker_palette(config: &Config, image: &str) {
    write_picker_palette_cols(config, &preview_colors(config, image));
}

fn run_native(image: &str, dark: bool) {
    let bin = crate::paths::sibling_bin("skwd-wall-scan");
    let mut cmd = Command::new(bin);
    cmd.arg("--theme").arg(image);
    if !dark {
        cmd.arg("--light");
    }
    let status = cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
    if let Err(err) = status {
        log::warn!("native theme helper failed: {err}");
    }
}

fn run_capture(cmd: &mut Command) -> Option<String> {
    let out = cmd.stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn native_preview(image: &str, dark: bool) -> Vec<String> {
    let bin = crate::paths::sibling_bin("skwd-wall-scan");
    let mut cmd = Command::new(bin);
    cmd.arg("--theme-preview").arg(image);
    if !dark {
        cmd.arg("--light");
    }
    run_capture(&mut cmd)
        .and_then(|text| serde_json::from_str::<Vec<String>>(text.trim()).ok())
        .unwrap_or_default()
}

fn matugen_preview_args(config: &Config, image: &str, dark: bool) -> Vec<String> {
    vec![
        "image".to_string(),
        image.to_string(),
        "--dry-run".to_string(),
        "-j".to_string(),
        "hex".to_string(),
        "-t".to_string(),
        config.theme().matugen_scheme(),
        "-m".to_string(),
        if dark { "dark" } else { "light" }.to_string(),
        "--source-color-index".to_string(),
        config.theme().matugen_color_index().to_string(),
    ]
}

pub(crate) fn matugen_source(val: &serde_json::Value) -> Option<String> {
    val["colors"]["source_color"]["default"]["color"].as_str().map(str::to_string)
}

pub(crate) fn matugen_pick(val: &serde_json::Value) -> Vec<String> {
    let cols = &val["colors"];
    ["primary", "tertiary", "surface_variant", "surface_container", "surface", "outline"]
        .iter()
        .filter_map(|key| cols[*key]["default"]["color"].as_str().map(String::from))
        .collect()
}

fn matugen_preview(config: &Config, image: &str, dark: bool) -> Vec<String> {
    let mut cmd = Command::new("matugen");
    cmd.args(matugen_preview_args(config, image, dark));
    let Some(out) = run_capture(&mut cmd) else {
        return Vec::new();
    };
    let Ok(val) = serde_json::from_str::<serde_json::Value>(&out) else {
        return Vec::new();
    };
    matugen_pick(&val)
}

fn wallust_preview(config: &Config, image: &str, dark: bool) -> Vec<String> {
    let tmp = std::env::temp_dir().join(format!("skwd-wallust-{}", crate::paths::tmp_suffix()));
    let cfgdir = tmp.join("wallust");
    let cache = tmp.join("cache");
    std::fs::create_dir_all(&cfgdir).ok();
    std::fs::create_dir_all(&cache).ok();
    std::fs::write(cfgdir.join("wallust.toml"), "").ok();
    let mut cmd = Command::new("wallust");
    cmd.env("XDG_CACHE_HOME", &cache)
        .env("XDG_CONFIG_HOME", &tmp)
        .arg("run")
        .arg(image)
        .arg("--print-scheme")
        .arg("--skip-templates")
        .arg("--skip-sequences")
        .arg("--no-cache")
        .arg("--config-dir")
        .arg(&cfgdir)
        .arg("--no-hooks");
    if let Some(pal) = config.theme().wallust_palette() {
        cmd.arg("-p").arg(pal);
    } else if !dark {
        cmd.arg("-p").arg("light16");
    }
    if let Some(cs) = config.theme().wallust_colorspace() {
        cmd.arg("--colorspace").arg(cs);
    }
    let out = run_capture(&mut cmd);
    std::fs::remove_dir_all(&tmp).ok();
    out.map(|text| wallust_pick_from_text(&text)).unwrap_or_default()
}

fn wallust_pick_from_text(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| line.len() == 7 && line.starts_with('#'))
        .collect();
    wallust_pick(&lines)
}

fn wallust_pick(lines: &[&str]) -> Vec<String> {
    if lines.len() < WALLUST_MIN_COLORS {
        return Vec::new();
    }
    [1usize, 2, 4, 8, 0, 7]
        .iter()
        .filter_map(|&idx| lines.get(idx).map(|line| (*line).to_string()))
        .collect()
}

fn xdg_cache_home() -> std::path::PathBuf {
    if let Some(dir) = std::env::var_os("XDG_CACHE_HOME").filter(|val| !val.is_empty()) {
        return std::path::PathBuf::from(dir);
    }
    let home = std::env::var_os("HOME").unwrap_or_default();
    std::path::PathBuf::from(home).join(".cache")
}

fn pywal_colors_from_cache() -> Vec<String> {
    let path = xdg_cache_home().join("wal").join("colors.json");
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .map(|val| pywal_pick(&val))
        .unwrap_or_default()
}

fn pywal_pick(val: &serde_json::Value) -> Vec<String> {
    let cols = &val["colors"];
    let mut out = Vec::new();
    for key in ["color1", "color4", "color2", "color6"] {
        if let Some(hex) = cols[key].as_str() {
            out.push(hex.to_string());
        }
    }
    if let Some(hex) = val["special"]["background"].as_str() {
        out.push(hex.to_string());
    }
    if let Some(hex) = cols["color8"].as_str() {
        out.push(hex.to_string());
    }
    out
}

fn pywal_preview(config: &Config, image: &str, dark: bool) -> Vec<String> {
    let tmp = std::env::temp_dir().join(format!("skwd-wal-{}", crate::paths::tmp_suffix()));
    std::fs::create_dir_all(&tmp).ok();
    let mut cmd = Command::new("wal");
    cmd.env("XDG_CACHE_HOME", &tmp)
        .arg("-i")
        .arg(image)
        .arg("-n")
        .arg("-s")
        .arg("-t")
        .arg("-e")
        .arg("-q");
    if let Some(sat) = config.theme().pywal_saturate() {
        cmd.arg("--saturate").arg(sat);
    }
    if !dark {
        cmd.arg("-l");
    }
    let ran = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|st| st.success());
    let out = if ran {
        std::fs::read_to_string(tmp.join("wal").join("colors.json"))
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .map(|val| pywal_pick(&val))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    std::fs::remove_dir_all(&tmp).ok();
    out
}

fn parse_seed(hex: &str) -> Option<skwd_palette::Rgb> {
    let digits = hex.trim().strip_prefix('#')?;
    if digits.len() != 6 {
        return None;
    }
    let chan = |idx: usize| u8::from_str_radix(digits.get(idx..idx + 2)?, 16).ok();
    Some(skwd_palette::Rgb(chan(0)?, chan(2)?, chan(4)?))
}

pub fn static_palette_value(config: &Config, dark: bool) -> Option<serde_json::Value> {
    let name = config.theme().static_theme();
    if name == "custom" {
        let seeds: Vec<skwd_palette::Rgb> =
            config.theme().static_custom().iter().filter_map(|hex| parse_seed(hex)).collect();
        if seeds.is_empty() {
            return None;
        }
        return Some(skwd_palette::derive(&seeds, dark).to_value());
    }
    if let Some(saved) =
        config.theme().saved_themes().into_iter().find(|theme| {
            theme.get("name").and_then(serde_json::Value::as_str) == Some(name.as_str())
        })
    {
        let mut val = saved;
        if let Some(map) = val.as_object_mut() {
            map.remove("name");
            let alias =
                |map: &mut serde_json::Map<String, serde_json::Value>, from: &str, to: &str| {
                    if let Some(node) = map.get(from).cloned() {
                        map.entry(to.to_string()).or_insert(node);
                    }
                };
            alias(map, "primaryText", "on_primary");
            alias(map, "surfaceText", "on_surface");
        }
        return Some(val);
    }
    skwd_palette::preset(&name).map(|pal| pal.to_value())
}

fn static_preview(config: &Config, dark: bool) -> Vec<String> {
    let Some(val) = static_palette_value(config, dark) else {
        return Vec::new();
    };
    ["primary", "tertiary", "surfaceVariant", "surfaceContainer", "surface", "outline"]
        .iter()
        .filter_map(|key| val.get(*key).and_then(serde_json::Value::as_str).map(str::to_string))
        .collect()
}

fn direct_palette(config: &Config, image: &str, dark: bool) -> Option<(serde_json::Value, String)> {
    match resolve_backend(config).as_str() {
        "skwd-pywal" | "skwd-wallust" => {
            let variant = if resolve_backend(config) == "skwd-wallust" {
                config.theme().wallust_palette().unwrap_or_else(|| "dark16".to_string())
            } else {
                "dark16".to_string()
            };
            ansi16_run(image, dark, &variant)
                .and_then(|cols| ansi_palette(&cols).map(|palette| (palette, cols[4].clone())))
        }
        _ => None,
    }
}

fn backend_preview_colors(config: &Config, image: &str, dark: bool) -> Vec<String> {
    let colors = match resolve_backend(config).as_str() {
        "static" => static_preview(config, dark),
        "matugen" => matugen_preview(config, image, dark),
        "wallust" => wallust_preview(config, image, dark),
        "pywal" => pywal_preview(config, image, dark),
        "skwd-iris" => skwd_iris_preview(image, dark),
        "skwd-pywal" | "skwd-wallust" => ansi_preview(image, dark),
        "iris" => iris_preview(image, dark),
        _ => native_preview(image, dark),
    };
    if colors.is_empty() { native_preview(image, dark) } else { colors }
}

pub fn styled_palette(config: &Config, seed: &str, dark: bool) -> Option<serde_json::Value> {
    let doc = crate::material::document_with(seed, dark, &config.theme().scheme())?;
    let palette = palette_from_scheme(&doc)?;
    Some(crate::style::restyle(&palette, &config.theme().style()))
}

pub fn preview_palette(config: &Config, image: &str) -> Option<serde_json::Value> {
    let dark = resolve_dark(config, image);
    match resolve_backend(config).as_str() {
        "noctalia" => return crate::noctalia::preview_palette(config, image, dark),
        "dms" => return crate::dms::preview_palette(config, image, dark),
        _ => {}
    }
    if let Some((palette, _)) = direct_palette(config, image, dark) {
        return Some(crate::style::restyle(&palette, &config.theme().style()));
    }
    let colors = backend_preview_colors(config, image, dark);
    colors.first().and_then(|seed| styled_palette(config, seed, dark)).or_else(|| {
        Some(crate::style::restyle(&picker_palette_json(&colors)?, &config.theme().style()))
    })
}

pub fn preview_palettes(config: &Config, image: &str) -> Vec<(String, serde_json::Value)> {
    let dark = resolve_dark(config, image);
    let seed = direct_palette(config, image, dark)
        .map(|(_, seed)| seed)
        .or_else(|| backend_preview_colors(config, image, dark).into_iter().next());
    let Some(seed) = seed else {
        return Vec::new();
    };
    crate::material::SCHEMES
        .iter()
        .filter_map(|scheme| {
            let document = crate::material::document_with(&seed, dark, scheme)?;
            let palette = palette_from_scheme(&document)?;
            Some(((*scheme).to_string(), crate::style::restyle(&palette, &config.theme().style())))
        })
        .collect()
}

pub fn preview_colors(config: &Config, image: &str) -> Vec<String> {
    let Some(palette) = preview_palette(config, image) else {
        return Vec::new();
    };
    let swatch = swatch_from_palette(&palette);
    if swatch.len() == SWATCH_KEYS.len() {
        swatch
    } else {
        backend_preview_colors(config, image, resolve_dark(config, image))
    }
}

pub fn tone_dark(image: &str) -> Option<bool> {
    let bin = crate::paths::sibling_bin("skwd-wall-scan");
    let mut cmd = Command::new(bin);
    cmd.arg("--tone").arg(image);
    run_capture(&mut cmd)
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|val| val.get("dark").and_then(serde_json::Value::as_bool))
}

const TONE_CACHE_CAP: usize = 512;

pub fn resolve_dark(config: &Config, image: &str) -> bool {
    static CACHE: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();
    let mode = config.theme().mode();
    if mode != "auto" {
        return mode != "light";
    }
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(hit) = lock(cache).get(image) {
        return *hit;
    }
    let dark = tone_dark(image).unwrap_or_else(|| {
        log::warn!("auto mode: could not read {image}, defaulting to dark");
        true
    });
    let mut map = lock(cache);
    if map.len() >= TONE_CACHE_CAP {
        map.clear();
    }
    map.insert(image.to_string(), dark);
    dark
}

pub fn apply(config: &Config, image_path: &str) -> bool {
    let backend = resolve_backend(config);
    let dark = resolve_dark(config, image_path);
    log::info!("theme apply: backend={backend} dark={dark} src={image_path}");
    match backend.as_str() {
        "off" => true,
        "static" => {
            let Some(val) = static_palette_value(config, dark) else {
                log::warn!("static theme selected but no palette is configured; nothing applied");
                return false;
            };
            let json = val.to_string();
            let native = config.theme().native_colors_path();
            if let Err(err) = crate::paths::atomic_write(&native, json.as_bytes()) {
                log::warn!("static theme: write {} failed: {err}", native.display());
            }
            let bridge = std::path::PathBuf::from(config.cache_dir()).join("colors.json");
            if let Err(err) = crate::paths::atomic_write(&bridge, json.as_bytes()) {
                log::warn!("static theme: write {} failed: {err}", bridge.display());
            }
            if let Some(seed) = val.get("primary").and_then(serde_json::Value::as_str) {
                write_scheme(config, seed, dark);
            }
            crate::static_templates::render_integrations(config, &val, dark);
            run_reloads(config);
            true
        }
        "matugen" => crate::matugen::run(config, image_path),
        "noctalia" => {
            let ok = crate::noctalia::write_bridge_palette(config, image_path, dark);
            if ok {
                run_reloads(config);
            }
            ok
        }
        "dms" => {
            let ok = crate::dms::write_bridge_palette(config, image_path, dark);
            if ok {
                run_reloads(config);
            }
            ok
        }
        "caelestia" | "end4" => {
            let ok = crate::theme_provider::apply_authority(config, &backend, image_path, dark);
            if ok {
                run_reloads(config);
            }
            ok
        }
        "wallust" => {
            let mut sh = Command::new("sh");
            sh.arg("-c")
                .arg(format!("{} --print-scheme", wallust_command(config, image_path, dark)));
            let cols =
                run_capture(&mut sh).map(|text| wallust_pick_from_text(&text)).unwrap_or_default();
            publish_colors(config, &cols, dark, image_path);
            run_reloads(config);
            true
        }
        "iris" => {
            let mut cmd = Command::new("iris");
            cmd.args(iris_args(image_path, dark, false));
            if run_capture(&mut cmd).is_none() {
                log::warn!("iris failed for {image_path}");
            }
            publish_colors(config, &iris_colors_from_cache(), dark, image_path);
            run_reloads(config);
            true
        }
        "skwd-iris" => {
            publish_colors(config, &skwd_iris_preview(image_path, dark), dark, image_path);
            run_reloads(config);
            true
        }
        "skwd-pywal" | "skwd-wallust" => {
            if let Some((palette, seed)) = direct_palette(config, image_path, dark) {
                let styled = write_picker_palette_value(config, &palette);
                write_scheme(config, &seed, dark);
                crate::static_templates::render_integrations(config, &styled, dark);
            } else {
                log::warn!("{backend}: no colours extracted from {image_path}");
                publish_colors(config, &[], dark, image_path);
            }
            run_reloads(config);
            true
        }
        "pywal" => {
            run_sh(&pywal_command(config, image_path, dark));
            let cols = pywal_colors_from_cache();
            publish_colors(config, &cols, dark, image_path);
            run_reloads(config);
            true
        }
        _ => {
            run_native(image_path, dark);
            let cols = native_swatch_from_disk(config);
            publish_colors(config, &cols, dark, image_path);
            run_reloads(config);
            true
        }
    }
}

#[path = "tests.rs"]
mod tests;
