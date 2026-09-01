use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod we_property_kind {
    pub const BOOL: &str = "bool";
    pub const COLOR: &str = "color";
    pub const SLIDER: &str = "slider";
    pub const COMBO: &str = "combo";
    pub const GROUP: &str = "group";
    pub const UNSUPPORTED: &str = "unsupported";
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct WePropertyOption {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub value: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct WeProperty {
    pub name: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub declared: String,
    #[serde(default)]
    pub value: Value,
    #[serde(default)]
    pub default: Value,
    #[serde(default)]
    pub overridden: bool,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub step: Option<f64>,
    #[serde(default)]
    pub options: Vec<WePropertyOption>,
    #[serde(default)]
    pub condition: Option<String>,
    #[serde(default)]
    pub order: i64,
}

impl WeProperty {
    #[must_use]
    pub fn editable(&self) -> bool {
        matches!(
            self.kind.as_str(),
            we_property_kind::BOOL
                | we_property_kind::COLOR
                | we_property_kind::SLIDER
                | we_property_kind::COMBO
        )
    }
}

#[cfg(test)]
#[path = "we_property_tests.rs"]
mod tests;
