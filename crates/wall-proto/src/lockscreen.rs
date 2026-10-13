use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LockScreenTarget {
    Plasma,
    Hyprlock,
    Swaylock,
    Dms,
    Noctalia,
    Sddm,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LockScreenSetRequest {
    pub target: LockScreenTarget,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sddm_image: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LockScreenSetResult {
    pub ok: bool,
    pub target: LockScreenTarget,
    pub enabled: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SddmChoices {
    pub current: String,
    pub themes: Vec<String>,
}

#[cfg(test)]
#[path = "lockscreen_tests.rs"]
mod tests;
