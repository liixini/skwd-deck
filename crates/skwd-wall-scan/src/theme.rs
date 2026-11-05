use std::path::{Path, PathBuf};

use serde_json::Value;
use skwd_palette::studio;
use skwd_wall_core::{WallState, media};
use wall_proto::{StudioArea, StudioMethod, StudioResult};

pub(crate) fn preview_swatch(image: &str, dark: bool) -> Vec<String> {
    let Some(rgba) = media::load_rgba(Path::new(image), 256) else {
        return Vec::new();
    };
    skwd_wall_core::theme::swatch_from_palette(&skwd_palette::generate(&rgba, dark).to_value())
}

pub(crate) fn print_ansi16(image: &str, dark: bool, auto: bool, variant: &str) {
    let Some((rgba, width, height)) = media::load_rgba_sized(Path::new(image), 256) else {
        return;
    };
    let dark = if auto { skwd_palette::seed::tone(&rgba).prefers_dark() } else { dark };
    let Some(colors) = skwd_palette::semantic::ansi16_variant(&rgba, width, height, dark, variant)
    else {
        return;
    };
    let hexadecimal: Vec<String> = colors.iter().map(|color| color.hex()).collect();
    println!("{}", serde_json::json!({ "dark": dark, "colors": hexadecimal }));
}

pub(crate) fn print_tone(image: &str) {
    let Some(rgba) = media::load_rgba(Path::new(image), 128) else {
        return;
    };
    let tone = skwd_palette::seed::tone(&rgba);
    println!("{}", serde_json::json!({ "dark": tone.prefers_dark() }));
}

pub(crate) fn print_semantic(image: &str, dark: bool, auto: bool) {
    let Some((rgba, width, height)) = media::load_rgba_sized(Path::new(image), 256) else {
        return;
    };
    let dark = if auto { skwd_palette::seed::tone(&rgba).prefers_dark() } else { dark };
    let Some(semantic) = skwd_palette::semantic::semantic(&rgba, width, height, dark) else {
        return;
    };
    let hex = skwd_palette::Rgb::hex;
    let output = serde_json::json!({
        "bg": hex(semantic.bg),
        "surface": hex(semantic.surface),
        "fg": hex(semantic.fg),
        "dim": hex(semantic.dim),
        "accent": hex(semantic.accent),
        "dark": dark,
    });
    println!("{output}");
}

const SEED_CLUSTERS: usize = 8;

fn wire(made: &studio::Made, light: &studio::Made, key: &str) -> StudioMethod {
    let (mask, sectors) = match &made.shape {
        studio::Shape::Plain => (Vec::new(), Vec::new()),
        studio::Shape::Mask(points) => (points.clone(), Vec::new()),
        studio::Shape::Sectors(points) => (Vec::new(), points.clone()),
    };
    StudioMethod {
        key: key.to_string(),
        dark: made.palette.to_wire(),
        light: light.palette.to_wire(),
        mark: made.mark,
        mask,
        sectors,
    }
}

fn area_result(image: &str, area: StudioArea) -> Option<StudioResult> {
    let (rgba, width, height) = media::load_rgba_sized(Path::new(image), studio::AREA_EDGE)?;
    let bounds =
        studio::Area { left: area.left, top: area.top, right: area.right, bottom: area.bottom };
    let found = studio::area_patches(&rgba, width, height, bounds);
    let seed = studio::area_seed(&found)?;
    let dark = studio::area_palette(&found, true)?;
    let light = studio::area_palette(&found, false)?;
    Some(StudioResult {
        image: image.to_string(),
        seed: studio::from_oklch(seed.lch).hex(),
        grey: false,
        wheel: studio::area_wheel(&found).to_vec(),
        methods: vec![StudioMethod {
            key: studio::AREA_KEY.to_string(),
            dark: dark.to_wire(),
            light: light.to_wire(),
            mark: None,
            mask: Vec::new(),
            sectors: Vec::new(),
        }],
        materials: Vec::new(),
        dark: None,
    })
}

pub(crate) fn studio_result(image: &str, area: Option<StudioArea>) -> Option<StudioResult> {
    if let Some(area) = area {
        return area_result(image, area);
    }
    let (rgba, width, height) = media::load_rgba_sized(Path::new(image), studio::ANALYSIS_EDGE)?;
    let analysis = studio::Analysis::new(&rgba, width, height)?;
    let methods = studio::Method::ALL
        .iter()
        .map(|method| {
            wire(&method.make(&analysis, true), &method.make(&analysis, false), method.key())
        })
        .collect();
    Some(StudioResult {
        image: image.to_string(),
        seed: skwd_palette::seed::seed(&rgba, width, height, SEED_CLUSTERS)
            .unwrap_or_else(|| analysis.seed().color())
            .hex(),
        grey: analysis.grey,
        wheel: studio::wheel(&analysis).to_vec(),
        methods,
        materials: Vec::new(),
        dark: None,
    })
}

pub(crate) fn print_studio(image: &str, area: Option<StudioArea>) {
    if let Some(result) = studio_result(image, area) {
        println!("{}", serde_json::to_string(&result).unwrap_or_default());
    }
}

pub(crate) fn print_preview(image: &str, dark: bool) {
    let colors = preview_swatch(image, dark);
    println!("{}", serde_json::to_string(&colors).unwrap_or_else(|_| "[]".into()));
}

pub(crate) fn write_native(state: &WallState, image: &str, dark: bool) {
    if image.is_empty() {
        log::warn!("native theme: no image path");
        return;
    }
    let config = state.config();
    let Some(rgba) = media::load_rgba(Path::new(image), 256) else {
        log::warn!("native theme: failed to decode {image}");
        return;
    };
    let colors = skwd_palette::generate(&rgba, dark).to_value();

    let output = config.theme().native_colors_path();
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    match serde_json::to_string_pretty(&colors) {
        Ok(text) => match std::fs::write(&output, text) {
            Ok(()) => log::info!("native theme: wrote {}", output.display()),
            Err(error) => {
                log::warn!("native theme: write {}: {error}", output.display());
            }
        },
        Err(error) => log::warn!("native theme: serialize colors: {error}"),
    }
    fill_templates(&config, &colors);
}

pub(crate) fn render_template(template: &str, colors: &Value) -> String {
    let mut output = template.to_string();
    if let Some(map) = colors.as_object() {
        for (key, value) in map {
            if let Some(hexadecimal) = value.as_str() {
                output = output
                    .replace(&format!("{{{{{key}.strip}}}}"), hexadecimal.trim_start_matches('#'));
                output = output.replace(&format!("{{{{{key}}}}}"), hexadecimal);
            }
        }
    }
    output
}

fn fill_templates(config: &skwd_wall_core::config::Config, colors: &Value) {
    let template_directory = config.theme().templates_dir();
    let cache = PathBuf::from(config.cache_dir());
    for (template, output) in config.theme().native_templates() {
        let input = if template.contains('/') {
            PathBuf::from(config.resolve(&template))
        } else {
            template_directory.join(&template)
        };
        let Ok(template_text) = std::fs::read_to_string(&input) else {
            log::warn!("native theme: template not found: {}", input.display());
            continue;
        };
        let output = if output.contains('/') {
            PathBuf::from(config.resolve(&output))
        } else {
            cache.join(&output)
        };
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        if let Err(error) = std::fs::write(&output, render_template(&template_text, colors)) {
            log::warn!("native theme: write {}: {error}", output.display());
        }
    }
}
