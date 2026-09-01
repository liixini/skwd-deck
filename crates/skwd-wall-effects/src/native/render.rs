use std::path::{Path, PathBuf};

use image::DynamicImage;
use serde_json::Value;

use super::{builtin, storage};

#[cfg(test)]
pub(crate) async fn render_to_file(
    effect: &str,
    input: &Path,
    params: &Value,
    output: &Path,
    max_dimension: u32,
    preview: bool,
) -> anyhow::Result<PathBuf> {
    let effects = [serde_json::json!({ "effect": effect, "params": params })];
    render_effects_to_file(&effects, input, output, max_dimension, preview).await
}

pub(crate) async fn render_effects_to_file(
    effects: &[Value],
    input: &Path,
    output: &Path,
    max_dimension: u32,
    preview: bool,
) -> anyhow::Result<PathBuf> {
    if effects.is_empty() {
        anyhow::bail!("no effects requested");
    }
    let image = storage::load_working(input, max_dimension)?;
    let mut rendered = image;
    for step in effects {
        let effect = step
            .get("effect")
            .and_then(Value::as_str)
            .filter(|effect| !effect.is_empty())
            .ok_or_else(|| anyhow::anyhow!("effect step is missing an effect id"))?;
        let params = step.get("params").unwrap_or(&Value::Null);
        rendered = match crate::registry::find(effect) {
            Some(definition) => (definition.render)(rendered, params)?,
            None => render_sync(effect, rendered, params)?,
        };
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if preview {
        let final_path = output.with_extension("png");
        storage::write_fast_png(&rendered, &final_path)?;
        return Ok(final_path);
    }
    let final_path =
        output.with_extension(storage::target_ext(input, storage::has_alpha(&rendered)));
    storage::write_image(&rendered, &final_path, storage::source_is_lossless_webp(input))?;
    Ok(final_path)
}

pub(crate) fn render_sync(
    effect: &str,
    image: DynamicImage,
    params: &Value,
) -> anyhow::Result<DynamicImage> {
    builtin::render(effect, image, params)
}
