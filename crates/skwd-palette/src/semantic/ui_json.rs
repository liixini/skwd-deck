use super::layers::derive_ui_palette;
use super::scheme::Semantic;

pub fn ui_palette(semantic: &Semantic) -> serde_json::Value {
    let palette = derive_ui_palette(semantic);
    serde_json::json!({
        "primary": palette.primary.hex(),
        "primaryText": palette.primary_text.hex(),
        "tertiary": palette.tertiary.hex(),
        "surface": palette.surface.hex(),
        "surfaceText": palette.surface_text.hex(),
        "surfaceVariant": palette.surface_variant.hex(),
        "surfaceContainer": palette.surface_container.hex(),
        "background": palette.background.hex(),
        "outline": palette.outline.hex(),
    })
}

#[cfg(test)]
#[path = "ui_json_tests.rs"]
mod tests;
