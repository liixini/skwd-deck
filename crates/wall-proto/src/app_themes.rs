use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AppThemeSet {
    pub id: String,
    pub enabled: bool,
    #[serde(default)]
    pub adopt: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AppThemeRefresh {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integration: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct AppThemeStatus {
    pub id: String,
    pub name: String,
    pub installed: bool,
    pub config_found: bool,
    pub config_path: String,
    pub output_path: String,
    pub enabled: bool,
    pub state: String,
    pub detail: String,
    pub can_enable: bool,
    pub can_disable: bool,
    pub can_adopt: bool,
    #[serde(default)]
    pub template_path: String,
    #[serde(default)]
    pub customized: bool,
    #[serde(default)]
    pub can_disconnect: bool,
    #[serde(default)]
    pub can_reconnect: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct AppThemesResult {
    pub apps: Vec<AppThemeStatus>,
}

#[cfg(test)]
#[path = "app_themes_tests.rs"]
mod tests;
