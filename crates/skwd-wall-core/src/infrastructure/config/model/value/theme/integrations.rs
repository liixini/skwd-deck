use serde_json::Value;

use super::ThemeConfig;

pub struct Integration {
    pub name: String,
    pub template: String,
    pub output: String,
    pub reload: String,
    pub live_preview: bool,
}

impl ThemeConfig<'_> {
    pub fn disabled_integration(&self, name: &str) -> bool {
        self.config.get("integrations").and_then(Value::as_array).is_some_and(|entries| {
            entries.iter().any(|entry| {
                entry["name"].as_str().is_some_and(|id| id.eq_ignore_ascii_case(name))
                    && entry["enabled"] == false
            })
        })
    }

    pub fn integrations(&self) -> Vec<Integration> {
        self.config
            .get("integrations")
            .and_then(Value::as_array)
            .map(|integrations| {
                integrations
                    .iter()
                    .filter(|entry| entry.get("enabled").and_then(Value::as_bool) != Some(false))
                    .map(Self::integration)
                    .collect()
            })
            .unwrap_or_default()
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
