use std::path::PathBuf;

use serde::Deserialize;
use wall_proto::{LightPhase, OutdoorMatch, OutdoorSeason, WallpaperItem};

const DIRECTORY: &str = "outdoors";
const THUMB_EXTENSION: &str = "thumb.jpg";
const SOURCES_FILE: &str = "sources.json";
const SOURCES: &[u8] = include_bytes!("../../assets/outdoors/sources.json");
const MANIFEST: &str = include_str!("../../assets/outdoors/manifest.json");
#[path = "../../assets/outdoors/embedded.rs"]
mod assets;

#[derive(Deserialize)]
struct Manifest {
    entries: Vec<Entry>,
}

#[derive(Deserialize)]
pub(crate) struct Entry {
    pub file: String,
    pub name: String,
    pub season: OutdoorSeason,
    pub lights: Vec<LightPhase>,
    pub weather: Vec<String>,
    pub family: String,
    pub match_kind: OutdoorMatch,
    #[serde(default)]
    pub requires_snow: bool,
    #[serde(default)]
    pub requires_no_snow: bool,
    pub width: Option<i64>,
    pub height: Option<i64>,
}

impl Entry {
    pub(crate) fn matches(&self, context: &wall_rules::outdoors::Context) -> bool {
        (!self.requires_snow || context.snow)
            && (!self.requires_no_snow || !context.snow)
            && (self.season == OutdoorSeason::Neutral || self.season == context.season)
            && (self.lights.is_empty() || self.lights.contains(&context.light))
            && (self.weather.is_empty() || self.weather.contains(&context.category))
    }

    pub(crate) fn item(&self, cache: &str) -> WallpaperItem {
        let path = PathBuf::from(cache).join(DIRECTORY).join(&self.file);
        let thumb = path.with_extension(THUMB_EXTENSION);
        let thumb =
            if thumb.is_file() { thumb } else { path.clone() }.to_string_lossy().into_owned();
        let path = path.to_string_lossy().into_owned();
        WallpaperItem {
            key: Some(format!("{}:{path}", wall_proto::kind::STATIC)),
            kind: Some(wall_proto::kind::STATIC.to_owned()),
            name: Some(self.name.clone()),
            preview: Some(thumb.clone()),
            thumb: Some(thumb),

            width: self.width,
            height: self.height,
            path: Some(path),
            ..WallpaperItem::default()
        }
    }
}

pub(crate) fn entries(cache: &str) -> std::io::Result<Vec<Entry>> {
    let root = PathBuf::from(cache).join(DIRECTORY);
    std::fs::create_dir_all(&root)?;
    let sources = root.join(SOURCES_FILE);
    if !std::fs::read(&sources).is_ok_and(|stored| stored == SOURCES) {
        skwd_config::atomic_write(&sources, SOURCES)?;
    }
    for (name, bytes) in assets::IMAGES {
        let path = root.join(name);
        if !std::fs::metadata(&path).is_ok_and(|stored| stored.len() == bytes.len() as u64) {
            skwd_config::atomic_write(&path, bytes)?;
        }
    }
    serde_json::from_str::<Manifest>(MANIFEST)
        .map(|manifest| manifest.entries)
        .map_err(std::io::Error::other)
}

pub(crate) fn items(config: &skwd_wall_core::config::Config) -> Vec<WallpaperItem> {
    let mut seen = std::collections::BTreeSet::new();
    entries(&config.cache_dir())
        .unwrap_or_default()
        .iter()
        .map(|entry| entry.item(&config.cache_dir()))
        .filter(|item| seen.insert(item.key.clone()))
        .collect()
}

pub(crate) fn credits(cache: &str) -> String {
    PathBuf::from(cache).join(DIRECTORY).join(SOURCES_FILE).to_string_lossy().into_owned()
}
