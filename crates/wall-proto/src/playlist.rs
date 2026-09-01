use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PlaylistRow {
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub order: String,
    #[serde(default = "default_dwell")]
    pub dwell: i64,
    #[serde(default)]
    pub position: i64,
    #[serde(default)]
    pub count: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PlaylistAssign {
    pub output: String,
    pub id: i64,
}

fn default_dwell() -> i64 {
    600
}

#[cfg(test)]
#[path = "playlist_tests.rs"]
mod tests;
