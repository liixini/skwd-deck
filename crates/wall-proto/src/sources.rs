use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Wallhaven,
    Steam,
    Unsplash,
    Pexels,
    Youtube,
    Bing,
}

impl Provider {
    pub const fn key(self) -> &'static str {
        match self {
            Self::Wallhaven => "wallhaven",
            Self::Steam => "steam",
            Self::Unsplash => "unsplash",
            Self::Pexels => "pexels",
            Self::Youtube => "youtube",
            Self::Bing => "bing",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "wallhaven" => Some(Self::Wallhaven),
            "steam" => Some(Self::Steam),
            "unsplash" => Some(Self::Unsplash),
            "pexels" => Some(Self::Pexels),
            "youtube" => Some(Self::Youtube),
            "bing" => Some(Self::Bing),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Media {
    Image,
    Video,
    Scene,
}

impl Media {
    pub fn apply_kind(self) -> &'static str {
        match self {
            Media::Image => "static",
            Media::Video => "video",
            Media::Scene => "we",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Generic,
    Native,
}

#[derive(Debug, Clone, Copy)]
pub struct SourceSpec {
    pub provider: Provider,
    pub key: &'static str,
    pub label: &'static str,
    pub media: Media,
    pub transport: Transport,
    pub searchable: bool,
}

pub const SOURCES: [SourceSpec; 6] = [
    SourceSpec {
        provider: Provider::Wallhaven,
        key: "wallhaven",
        label: "Wallhaven",
        media: Media::Image,
        transport: Transport::Native,
        searchable: true,
    },
    SourceSpec {
        provider: Provider::Steam,
        key: "steam",
        label: "Steam Workshop",
        media: Media::Scene,
        transport: Transport::Native,
        searchable: true,
    },
    SourceSpec {
        provider: Provider::Unsplash,
        key: "unsplash",
        label: "Unsplash",
        media: Media::Image,
        transport: Transport::Generic,
        searchable: true,
    },
    SourceSpec {
        provider: Provider::Pexels,
        key: "pexels",
        label: "Pexels",
        media: Media::Image,
        transport: Transport::Generic,
        searchable: true,
    },
    SourceSpec {
        provider: Provider::Youtube,
        key: "youtube",
        label: "YouTube",
        media: Media::Video,
        transport: Transport::Generic,
        searchable: true,
    },
    SourceSpec {
        provider: Provider::Bing,
        key: "bing",
        label: "Bing Daily",
        media: Media::Image,
        transport: Transport::Generic,
        searchable: false,
    },
];

pub fn spec(key: &str) -> Option<&'static SourceSpec> {
    SOURCES.iter().find(|src| src.key == key)
}

pub fn keys() -> impl Iterator<Item = &'static str> {
    SOURCES.iter().map(|src| src.key)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListItem {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub full_url: String,
    #[serde(default)]
    pub thumb_url: String,
    #[serde(default)]
    pub thumb_path: String,
    #[serde(default)]
    pub resolution: String,
    #[serde(default)]
    pub file_size: u64,
    #[serde(default)]
    pub purity: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub attribution: String,
    #[serde(default)]
    pub attribution_url: String,
    #[serde(default)]
    pub track_url: String,
    #[serde(default, rename = "duration")]
    pub duration_secs: u64,
    #[serde(default)]
    pub downloaded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<u64>,
    #[serde(default)]
    pub results: Vec<ListItem>,
    #[serde(default = "default_page")]
    pub last_page: u32,
    #[serde(default = "default_page")]
    pub current_page: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

impl Default for ListResult {
    fn default() -> Self {
        Self {
            generation: None,
            results: Vec::new(),
            last_page: default_page(),
            current_page: default_page(),
            next_cursor: None,
        }
    }
}

const fn default_page() -> u32 {
    1
}

mod tests;
