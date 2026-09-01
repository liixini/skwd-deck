use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WallpaperItem {
    pub key: Option<String>,
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub preview: Option<String>,
    pub thumb: Option<String>,
    pub thumb_sm: Option<String>,
    pub favourite: Option<i64>,
    pub hue: Option<i64>,
    pub sat: Option<i64>,
    pub tags: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_tags: Option<String>,
    pub colors: Option<String>,
    pub matugen: Option<String>,
    pub video_file: Option<String>,
    pub we_id: Option<String>,
    pub analyzed_by: Option<String>,
    pub filesize: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_ms: Option<i64>,
    pub mtime: Option<i64>,
    pub weather: Option<String>,
    pub richness: Option<i64>,
    pub apply_count: Option<i64>,
    pub last_applied: Option<i64>,
}
