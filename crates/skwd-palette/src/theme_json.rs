use crate::ThemePalette;

impl ThemePalette {
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::json!({
            "primary": self.primary.hex(),
            "primaryText": self.on_primary.hex(),
            "on_primary": self.on_primary.hex(),
            "surface": self.surface.hex(),
            "surfaceText": self.on_surface.hex(),
            "on_surface": self.on_surface.hex(),
            "surfaceVariant": self.surface_variant.hex(),
            "surfaceContainer": self.surface_container.hex(),
            "background": self.background.hex(),
            "outline": self.outline.hex(),
            "tertiary": self.tertiary.hex(),
        })
    }
}

#[cfg(test)]
#[path = "theme_json_tests.rs"]
mod tests;
