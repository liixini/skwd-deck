use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WorkspaceRow {
    pub output: String,
    #[serde(default)]
    pub idx: u64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub active: bool,
}

#[cfg(test)]
#[path = "workspace_tests.rs"]
mod tests;
