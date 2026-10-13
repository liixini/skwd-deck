use serde::{Deserialize, Serialize};

#[derive(Default, Deserialize, Serialize)]
pub struct WallRemoveParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub we_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unsubscribe: Option<bool>,
}

#[derive(Deserialize, Serialize)]
pub struct WallRemoveResult {
    pub removed: String,
    #[serde(default)]
    pub unsubscribed: bool,
}

#[cfg(test)]
#[path = "removal_tests.rs"]
mod tests;
