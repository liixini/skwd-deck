use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::Value;
use skwd_config::{FolderState, LibraryItem, LibraryProfile};
use skwd_wall_core::{WallState, db};
use wall_rules::playlist::{Filter, matches_item};

pub(crate) struct Visibility {
    wallpaper: PathBuf,
    videos: PathBuf,
    profile: Option<LibraryProfile>,
}

impl Visibility {
    pub(crate) fn load(state: &Arc<WallState>) -> Self {
        let cfg = state.config();
        Self {
            wallpaper: PathBuf::from(cfg.wallpaper_dir()),
            videos: PathBuf::from(cfg.video_dir()),
            profile: cfg.library_profile(),
        }
    }

    pub(crate) fn scoped(&self) -> bool {
        self.profile.is_some()
    }

    pub(crate) fn shows(&self, key: &str, name: &str, video: &str) -> bool {
        let path = self.entry_path(key, name, video);
        let item = LibraryItem::new(key, &path);
        skwd_config::profile_state(self.profile.as_ref(), item) == FolderState::Show
    }

    fn entry_path(&self, key: &str, name: &str, video: &str) -> String {
        db::entry_path(&self.wallpaper, db::key_kind(key), name, video)
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    pub(crate) fn matches(&self, item: &Value, filter: &Filter) -> bool {
        let path = if filter.has_folders() { self.folder_path(item) } else { String::new() };
        matches_item(item, &path, filter)
    }

    fn folder_path(&self, item: &Value) -> String {
        let Ok(item) = wall_proto::WallpaperItem::deserialize(item) else {
            return String::new();
        };
        let key = item.key.as_deref().unwrap_or_default();
        let name = item.name.as_deref().unwrap_or_default();
        let video = match item.video_file.as_deref() {
            Some(file) if !file.is_empty() => PathBuf::from(file),
            _ => self.videos.join(name),
        };
        let path = self.entry_path(key, name, &video.to_string_lossy());
        skwd_config::resolve_folder(std::path::Path::new(&path))
            .map_or(path, |path| path.to_string_lossy().into_owned())
    }

    pub(crate) fn shows_item(&self, item: &Value) -> bool {
        let Ok(item) = wall_proto::WallpaperItem::deserialize(item) else {
            return false;
        };
        !item.hidden
            && self.shows(
                item.key.as_deref().unwrap_or_default(),
                item.name.as_deref().unwrap_or_default(),
                item.video_file.as_deref().unwrap_or_default(),
            )
    }
}

#[cfg(test)]
mod tests;
