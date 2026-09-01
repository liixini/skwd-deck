use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DownloadEvent {
    pub id: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl DownloadEvent {
    pub fn new(id: &str, status: &str) -> Self {
        Self { id: id.to_string(), status: status.to_string(), ..Self::default() }
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

pub mod dl_status {
    pub const QUEUED: &str = "queued";
    pub const DOWNLOADING: &str = "downloading";
    pub const DONE: &str = "done";
    pub const ERROR: &str = "error";
    pub const AUTH_ERROR: &str = "auth_error";
}

#[cfg(test)]
#[path = "download_tests.rs"]
mod tests;
