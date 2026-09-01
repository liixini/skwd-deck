use std::path::PathBuf;

use serde_json::Value;

use super::Config;
use crate::infrastructure::config::config_path;

pub struct Integration {
    pub name: String,
    pub template: String,
    pub output: String,
    pub reload: String,
    pub live_preview: bool,
}

#[derive(Clone, Copy)]
pub struct ThemeConfig<'a> {
    config: &'a Config,
}

impl<'a> ThemeConfig<'a> {
    pub(super) fn new(config: &'a Config) -> Self {
        Self { config }
    }

    fn root(&self) -> &Value {
        self.config.root()
    }

    skwd_config::getters! {
        dms_hover_preview: bool(skwd_config::keys::dms::HOVER_PREVIEW, true);
        matugen_enabled: on_unless_off(skwd_config::keys::features::MATUGEN);
        matugen_mode: str(skwd_config::keys::matugen::MODE, "dark");
        matugen_scheme: str(skwd_config::keys::matugen::SCHEME_TYPE, "scheme-fidelity");
        noctalia_hover_preview: bool(skwd_config::keys::noctalia::HOVER_PREVIEW, true);
        noctalia_pure_black: bool(skwd_config::keys::theme::NOCTALIA_PURE_BLACK, false);
        static_theme: str(skwd_config::keys::theme::STATIC_THEME, "nord");
    }

    pub fn backend(&self) -> String {
        skwd_config::theme_backend(self.root())
    }

    pub fn policy(&self) -> String {
        skwd_config::theme_policy(self.root())
    }

    pub fn authority(&self) -> String {
        skwd_config::theme_authority(self.root())
    }

    pub fn engine(&self) -> String {
        skwd_config::theme_engine(self.root())
    }

    pub fn targets(&self) -> Vec<String> {
        let mut targets = Vec::new();
        for target in self
            .config
            .get(skwd_config::keys::theme::TARGETS)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if matches!(target, "caelestia" | "dms" | "noctalia" | "end4")
                && !targets.iter().any(|item| item == target)
            {
                targets.push(target.to_string());
            }
        }
        targets
    }

    pub fn saved_themes(&self) -> Vec<Value> {
        self.config
            .get(skwd_config::keys::theme::SAVED_THEMES)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    }

    pub fn static_custom(&self) -> Vec<String> {
        match self.config.get(skwd_config::keys::theme::CUSTOM_COLORS) {
            Some(Value::Array(colors)) => {
                colors.iter().filter_map(|value| value.as_str().map(str::to_string)).collect()
            }
            Some(Value::String(colors)) => colors
                .split(',')
                .map(|color| color.trim().to_string())
                .filter(|color| !color.is_empty())
                .collect(),
            _ => Vec::new(),
        }
    }

    pub fn scheme(&self) -> String {
        let value = self.config.str_at(skwd_config::keys::theme::SCHEME, "");
        if value.is_empty() { "tonal-spot".to_string() } else { value }
    }

    pub fn style(&self) -> String {
        let value = self.config.str_at(skwd_config::keys::theme::STYLE, "");
        if value.is_empty() { "natural".to_string() } else { value }
    }

    pub fn mode(&self) -> String {
        let mode = self.config.str_at(skwd_config::keys::theme::MODE, "");
        if mode.is_empty() { self.matugen_mode() } else { mode }
    }

    pub fn noctalia_bin(&self) -> String {
        self.resolved_optional_path(skwd_config::keys::paths::NOCTALIA_BIN)
    }

    pub fn caelestia_bin(&self) -> String {
        self.resolved_optional_path(skwd_config::keys::paths::CAELESTIA_BIN)
    }

    pub fn wallust_palette(&self) -> Option<String> {
        self.optional_string(skwd_config::keys::theme::WALLUST_PALETTE)
    }

    pub fn wallust_colorspace(&self) -> Option<String> {
        self.optional_string(skwd_config::keys::theme::WALLUST_COLORSPACE)
    }

    pub fn pywal_saturate(&self) -> Option<String> {
        let value = self.config.str_at(skwd_config::keys::theme::PYWAL_SATURATE, "");
        value
            .parse::<f32>()
            .ok()
            .filter(|saturation| (0.0..=1.0).contains(saturation))
            .map(|_| value)
    }

    pub fn noctalia_scheme_override(&self) -> Option<String> {
        self.optional_string(skwd_config::keys::theme::NOCTALIA_SCHEME)
    }

    pub fn matugen_scheme_override(&self) -> Option<String> {
        self.optional_string(skwd_config::keys::matugen::SCHEME_TYPE)
    }

    pub fn native_colors_path(&self) -> PathBuf {
        let value = self.config.str_at(skwd_config::keys::theme::NATIVE_COLORS_PATH, "");
        if value.is_empty() {
            PathBuf::from(self.config.cache_dir()).join("skwd-colors.json")
        } else {
            PathBuf::from(self.config.resolve(&value))
        }
    }

    pub fn native_templates(&self) -> Vec<(String, String)> {
        self.config
            .get(skwd_config::keys::theme::NATIVE_TEMPLATES)
            .and_then(Value::as_array)
            .map(|templates| {
                templates
                    .iter()
                    .filter_map(|entry| {
                        let template = entry.get("template").and_then(Value::as_str)?;
                        let output = entry.get("output").and_then(Value::as_str)?;
                        (!template.is_empty() && !output.is_empty())
                            .then(|| (template.to_string(), output.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn matugen_color_index(&self) -> u32 {
        skwd_config::u64_at(self.root(), skwd_config::keys::matugen::COLOR_INDEX)
            .map_or(0, |value| value.min(3) as u32)
    }

    pub fn matugen_contrast(&self) -> Option<f64> {
        self.config
            .get(skwd_config::keys::matugen::CONTRAST)
            .and_then(Value::as_f64)
            .filter(|contrast| contrast.is_finite() && *contrast != 0.0)
            .map(|contrast| contrast.clamp(-1.0, 1.0))
    }

    pub fn templates_dir(&self) -> PathBuf {
        let value = self.config.str_at(skwd_config::keys::paths::TEMPLATES, "");
        if !value.is_empty() {
            return PathBuf::from(self.config.resolve(&value));
        }
        config_path().parent().map_or_else(
            || PathBuf::from(self.config.cache_dir()).join("matugen").join("templates"),
            |directory| directory.join("matugen").join("templates"),
        )
    }

    pub fn integrations(&self) -> Vec<Integration> {
        self.config
            .get("integrations")
            .and_then(Value::as_array)
            .map(|integrations| integrations.iter().map(Self::integration).collect())
            .unwrap_or_default()
    }

    pub fn default_matugen_config(&self) -> Option<String> {
        let value = self.config.str_at("defaultMatugenConfig", "");
        if value.is_empty() { None } else { Some(self.config.resolve(&value)) }
    }

    pub fn external_matugen_command(&self) -> Option<String> {
        self.optional_string("externalMatugenCommand")
    }

    fn optional_string(self, key: &str) -> Option<String> {
        let value = self.config.str_at(key, "");
        if value.is_empty() { None } else { Some(value) }
    }

    fn resolved_optional_path(self, key: &str) -> String {
        self.optional_string(key).map_or_else(String::new, |value| self.config.resolve(&value))
    }

    fn integration(entry: &Value) -> Integration {
        let name = entry.get("name").and_then(Value::as_str).unwrap_or("").to_string();
        let live_preview = entry
            .get("livePreview")
            .and_then(Value::as_bool)
            .unwrap_or_else(|| !matches!(name.to_ascii_lowercase().as_str(), "kde" | "plasma"));
        Integration {
            name,
            template: entry.get("template").and_then(Value::as_str).unwrap_or("").to_string(),
            output: entry.get("output").and_then(Value::as_str).unwrap_or("").to_string(),
            reload: entry.get("reload").and_then(Value::as_str).unwrap_or("").to_string(),
            live_preview,
        }
    }
}
