use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const TRACKING_FILE: &str = "steam-tracking.json";
const TRACKING_MODE: u32 = 0o600;

#[derive(Default, Deserialize, Serialize)]
pub struct SteamTracking {
    pub account: String,
    pub source: PathBuf,
    pub seen: HashMap<String, PathBuf>,
    pub hidden: HashMap<String, PathBuf>,
    #[serde(default)]
    pub owned: HashMap<String, HashMap<String, PathBuf>>,
    #[serde(default)]
    pub active: HashMap<String, HashMap<String, PathBuf>>,
}

impl SteamTracking {
    pub fn load() -> Self {
        std::fs::read(crate::paths::cache_dir().join(TRACKING_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = crate::paths::cache_dir().join(TRACKING_FILE);
        std::fs::create_dir_all(path.parent().expect("cache file has parent"))?;
        let bytes = serde_json::to_vec(self).map_err(std::io::Error::other)?;
        crate::paths::atomic_write_mode(&path, &bytes, Some(TRACKING_MODE))
    }

    pub fn hides(&self, source: &Path, id: &str, item: &Path) -> bool {
        self.source == source
            && self
                .hidden
                .get(id)
                .is_some_and(|expected| item.canonicalize().ok().as_ref() == Some(expected))
    }

    pub fn subscribed(&self, source: &Path, id: &str, item: &Path) -> bool {
        self.source == source
            && self
                .seen
                .get(id)
                .is_some_and(|expected| item.canonicalize().ok().as_ref() == Some(expected))
    }
}

#[cfg(test)]
#[path = "tracking_tests.rs"]
mod tests;
