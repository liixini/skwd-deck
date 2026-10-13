use serde::{Deserialize, Serialize};

pub mod weather_category {
    pub const CLEAR: &str = "clear";
    pub const SUNNY: &str = "sunny";
    pub const CLOUDY: &str = "cloudy";
    pub const RAINY: &str = "rainy";
    pub const SNOWY: &str = "snowy";
    pub const STORMY: &str = "stormy";
    pub const FOGGY: &str = "foggy";
    pub const WINDY: &str = "windy";
    pub const PRIORITY: [&str; 7] = [STORMY, SNOWY, RAINY, FOGGY, WINDY, CLOUDY, CLEAR];
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WeatherState {
    #[default]
    Disabled,
    NoLocation,
    Loading,
    Unavailable,
    Unmapped,
    Missing,
    Empty,
    SearchUnavailable,
    Active,
    Paused,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WeatherSource {
    #[default]
    Playlists,
    Semantic,
    Outdoors,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WeatherSelection {
    #[default]
    Automatic,
    Approved,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct WeatherCandidates {
    pub category: String,
    pub query: Option<String>,
    pub member_keys: Vec<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct WeatherStatus {
    pub weather: Vec<String>,
    pub enabled: bool,
    pub status: WeatherState,
    pub source: WeatherSource,
    pub selection: WeatherSelection,
    pub category: Option<String>,
    pub query: Option<String>,
    pub playlist_id: Option<i64>,
    pub playlist_name: Option<String>,
    pub member_keys: Vec<String>,
    pub resolved_count: usize,
    pub fetched_at: Option<u64>,
    pub paused_outputs: Vec<String>,
    pub resume_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outdoors: Option<OutdoorStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location_key: Option<String>,
}

#[cfg(test)]
#[path = "weather_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutdoorSeason {
    #[default]
    Neutral,
    Spring,
    Summer,
    Autumn,
    Winter,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LightPhase {
    Dawn,
    #[default]
    Day,
    Dusk,
    Night,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutdoorMatch {
    Library,
    Starter,
    #[default]
    Ambient,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct OutdoorStatus {
    pub generation: u64,
    pub location: String,
    pub credits: Option<String>,
    pub season: OutdoorSeason,
    pub light: LightPhase,
    pub match_kind: OutdoorMatch,
    pub stale: bool,
    pub outputs: Vec<OutdoorOutput>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct OutdoorOutput {
    pub output: String,
    pub key: String,
    pub name: String,
    pub preview: Option<String>,
    pub held: bool,
}

pub fn weather_location_key(locale: &str, latitude: f64, longitude: f64) -> String {
    serde_json::to_string(&(locale, latitude, longitude)).expect("weather location serialization")
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct WeatherFollowRequest {
    pub enabled: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct WeatherOutputRequest {
    pub output: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct WeatherExcludeRequest {
    pub key: String,
}
