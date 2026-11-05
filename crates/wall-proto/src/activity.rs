use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Activity {
    pub id: String,
    pub name: String,
    pub current: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ActivitiesResult {
    pub supported: bool,
    pub current: String,
    pub activities: Vec<Activity>,
}

pub const ACTIVITY_PARAM: &str = "activity";

pub mod apply_fields {
    pub const APPLY_TOKEN: &str = "apply_token";
    pub const TYPE: &str = "type";
    pub const PATH: &str = "path";
    pub const WE_ID: &str = "we_id";
    pub const OUTPUT: &str = "output";
    pub const ALL_OUTPUTS: &str = "*";
    pub const MUTE: &str = "mute";
    pub const VOLUME: &str = "volume";
    pub const APPLIED: &str = "applied";
    pub const VISIBLE: &str = "visible";
    pub const REQUEST_ID: &str = "request_id";
    pub const OK: &str = "ok";
    pub const DETAIL: &str = "detail";
    pub const ASSIGNMENT: &str = "assignment";
}

#[cfg(test)]
#[path = "activity_tests.rs"]
mod tests;
