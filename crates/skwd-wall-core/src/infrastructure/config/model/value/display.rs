use serde_json::Value;

use super::Config;

#[derive(Clone, Copy)]
pub struct DisplayConfig<'a> {
    config: &'a Config,
}

impl<'a> DisplayConfig<'a> {
    pub(super) fn new(config: &'a Config) -> Self {
        Self { config }
    }

    fn root(&self) -> &Value {
        self.config.root()
    }

    skwd_config::getters! {
        fill_color: str(skwd_config::keys::display::FILL_COLOR, "000000ff");
        fill_mode: str(skwd_config::keys::display::FILL_MODE, "fill");
    }

    pub fn fill_override_for(&self, output: &str) -> Option<String> {
        self.config
            .get(skwd_config::keys::display::FILL_MODES)
            .and_then(|modes| modes.get(output))
            .and_then(Value::as_str)
            .filter(|mode| mode.parse::<wall_geom::FillMode>().is_ok())
            .map(String::from)
    }

    pub fn fill_mode_for(&self, output: &str) -> String {
        self.fill_override_for(output).unwrap_or_else(|| self.fill_mode())
    }

    pub fn output_locked(&self, output: &str) -> bool {
        self.config
            .get(skwd_config::keys::display::OUTPUT_LOCKS)
            .and_then(|locks| locks.get(output))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    pub fn locked_outputs(&self) -> Vec<String> {
        self.config
            .get(skwd_config::keys::display::OUTPUT_LOCKS)
            .and_then(Value::as_object)
            .map(|locks| {
                locks
                    .iter()
                    .filter(|(_, locked)| locked.as_bool().unwrap_or(false))
                    .map(|(output, _)| output.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn output_policies(&self) -> serde_json::Map<String, Value> {
        self.config
            .get(skwd_config::keys::display::OUTPUT_POLICIES)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
    }

    pub fn fill_modes_signature(&self) -> String {
        self.config
            .get(skwd_config::keys::display::FILL_MODES)
            .map(Value::to_string)
            .unwrap_or_default()
    }

    pub fn fill_overrides_active(&self) -> bool {
        let default = self.fill_mode();
        self.config
            .get(skwd_config::keys::display::FILL_MODES)
            .and_then(Value::as_object)
            .is_some_and(|modes| {
                modes.values().any(|mode| {
                    mode.as_str().is_some_and(|text| {
                        text != default && text.parse::<wall_geom::FillMode>().is_ok()
                    })
                })
            })
    }
}
