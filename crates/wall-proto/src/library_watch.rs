use serde::{Deserialize, Serialize};

pub mod mode {
    pub const NATIVE: &str = "native";
    pub const POLLING: &str = "polling";
    pub const RECOVERING: &str = "recovering";
    pub const UNAVAILABLE: &str = "unavailable";
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryWatchRootStatus {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_completed_sweep_unix_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_scan_requested_unix_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_successful_convergence_unix_ms: Option<u64>,
    #[serde(default)]
    pub pending_scans: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_poll_error: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryWatchStatus {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub degraded: bool,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_budget_per_root: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_successful_convergence_unix_ms: Option<u64>,
    #[serde(default)]
    pub roots: Vec<LibraryWatchRootStatus>,
}

#[cfg(test)]
#[path = "library_watch_tests.rs"]
mod tests;
