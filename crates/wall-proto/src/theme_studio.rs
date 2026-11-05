use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const STUDIO_ENGINE: &str = "skwd-studio";
pub const STUDIO_CAPABILITY: &str = "colour-studio";
pub const STUDIO_SCAN_FLAG: &str = "--studio";
pub const STUDIO_AREA_FLAG: &str = "--area";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub struct StudioArea {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl StudioArea {
    pub fn from_edges(edges: &[f32]) -> Option<Self> {
        let [left, top, right, bottom] = *edges else {
            return None;
        };
        (left < right && top < bottom).then_some(Self { left, top, right, bottom })
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct StudioRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area: Option<StudioArea>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub we_id: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub materials: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct StudioMethod {
    pub key: String,
    pub dark: Value,
    pub light: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark: Option<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mask: Vec<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sectors: Vec<[f32; 2]>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct StudioResult {
    pub image: String,
    pub seed: String,
    pub grey: bool,
    pub wheel: Vec<f32>,
    pub methods: Vec<StudioMethod>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub materials: Vec<StudioMethod>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dark: Option<bool>,
}

#[cfg(test)]
#[path = "theme_studio_tests.rs"]
mod tests;
