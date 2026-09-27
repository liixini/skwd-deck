use serde_json::Value;

use super::DisplayConfig;

impl DisplayConfig<'_> {
    pub fn background_for(&self, output: &str) -> paper_control::Background {
        let output = output.split(',').next().unwrap_or(output);
        let mode = self
            .config
            .get(skwd_config::keys::display::BACKGROUND_MODES)
            .and_then(|map| map.get(output))
            .and_then(Value::as_str);
        let override_active = matches!(mode, Some("color" | "blur"));
        let mode = if override_active {
            mode.unwrap_or("color").to_string()
        } else {
            self.config.str_at(skwd_config::keys::display::BACKGROUND_MODE, "color")
        };
        let color = if override_active {
            self.config
                .get(skwd_config::keys::display::BACKGROUND_COLORS)
                .and_then(|map| map.get(output))
                .and_then(Value::as_str)
                .map(str::to_string)
        } else {
            None
        }
        .unwrap_or_else(|| self.fill_color());
        let hex = color.strip_prefix('#').unwrap_or(&color);
        let mut rgb = [0; 3];
        if matches!(hex.len(), 6 | 8) && hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            for (index, channel) in rgb.iter_mut().enumerate() {
                *channel = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).unwrap_or(0);
            }
        }
        paper_control::Background { color: rgb, blur: mode == "blur" }
    }
    pub fn fill_modes_signature(&self) -> String {
        serde_json::json!([
            self.config.get(skwd_config::keys::display::FILL_MODES),
            self.fill_color(),
            self.config.get(skwd_config::keys::display::BACKGROUND_MODE),
            self.config.get(skwd_config::keys::display::BACKGROUND_MODES),
            self.config.get(skwd_config::keys::display::BACKGROUND_COLORS),
        ])
        .to_string()
    }
}
