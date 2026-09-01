use serde_json::Value;

use super::Config;

#[derive(Clone, Copy)]
pub struct TransitionConfig<'a> {
    config: &'a Config,
}

impl<'a> TransitionConfig<'a> {
    pub(super) fn new(config: &'a Config) -> Self {
        Self { config }
    }

    fn root(&self) -> &Value {
        self.config.root()
    }

    skwd_config::getters! {
        enabled: on_unless_off(skwd_config::keys::transition::ENABLED);
        sand_sharp: bool(skwd_config::keys::transition::SAND_SHARP, false);
    }

    pub fn sand_quality(&self) -> String {
        let quality = self.config.str_at(skwd_config::keys::transition::SAND_QUALITY, "auto");
        if quality == "full" || quality == "low" { quality } else { String::from("auto") }
    }

    pub fn sand_scope(&self) -> String {
        let scope = self.config.str_at(skwd_config::keys::transition::SAND_SCOPE, "all");
        if scope == "primary" { scope } else { String::from("all") }
    }

    pub fn scope(&self, shader: &str) -> String {
        let scope = self
            .config
            .get(skwd_config::keys::transition::SHADER_SCOPES)
            .and_then(Value::as_object)
            .and_then(|scopes| scopes.get(shader))
            .and_then(Value::as_str);
        match scope {
            Some("primary") => String::from("primary"),
            Some("all") => String::from("all"),
            _ if shader.starts_with("sand-") => self.sand_scope(),
            _ => String::from("all"),
        }
    }

    pub fn sand_primary(&self) -> String {
        self.config.str_at(skwd_config::keys::transition::SAND_PRIMARY, "")
    }

    pub fn sand_fps(&self) -> String {
        let fps = self
            .config
            .get(skwd_config::keys::transition::SAND_FPS)
            .and_then(Value::as_f64)
            .map_or(0, |number| number.max(0.0) as u32);
        if fps > 0 { fps.to_string() } else { String::from("auto") }
    }

    pub fn shader(&self) -> String {
        let shader = self.config.str_at(skwd_config::keys::transition::SHADER, "random");
        if shader.is_empty() { "random".to_string() } else { shader }
    }

    pub fn duration_ms(&self) -> u64 {
        self.config
            .get(skwd_config::keys::transition::DURATION_MS)
            .and_then(Value::as_f64)
            .map_or(600, |number| number.clamp(50.0, 10000.0) as u64)
    }

    pub fn active(&self) -> bool {
        self.enabled() && !self.config.renderer().performance_mode()
    }
}
