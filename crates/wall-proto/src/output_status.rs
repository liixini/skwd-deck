use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OutputStatus {
    pub name: String,
    #[serde(default)]
    pub target: String,
    #[serde(default = "default_connected")]
    pub connected: bool,
    #[serde(default)]
    pub width: i32,
    #[serde(default)]
    pub height: i32,
    #[serde(default)]
    pub logical_width: i32,
    #[serde(default)]
    pub logical_height: i32,
    #[serde(default)]
    pub current: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub we_id: String,
    #[serde(default)]
    pub mute: bool,
    #[serde(default = "default_volume")]
    pub volume: u32,
    #[serde(default)]
    pub fill: String,
    #[serde(default, rename = "audioShared")]
    pub audio_shared: bool,
}

impl OutputStatus {
    pub fn target(&self) -> &str {
        if self.target.is_empty() { &self.name } else { &self.target }
    }

    pub fn is_connected(&self) -> bool {
        self.connected || self.target.is_empty()
    }

    pub fn logical_size(&self) -> (i32, i32) {
        if self.logical_width > 0 && self.logical_height > 0 {
            (self.logical_width, self.logical_height)
        } else {
            (self.width, self.height)
        }
    }
}

fn default_volume() -> u32 {
    100
}

fn default_connected() -> bool {
    true
}

pub fn outputs_from(result: &serde_json::Value) -> Vec<OutputStatus> {
    result
        .get("outputs")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "output_status_tests.rs"]
mod tests;
