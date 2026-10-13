use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

pub const EVERYTHING_PROFILE: &str = "everything";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FolderState {
    Hide,
    Exclude,
    #[default]
    #[serde(other)]
    Show,
}

impl FolderState {
    pub const fn key(self) -> &'static str {
        match self {
            Self::Show => "show",
            Self::Hide => "hide",
            Self::Exclude => "exclude",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryItem<'a> {
    Folder(&'a Path),
    Workshop,
    Other,
}

impl<'a> LibraryItem<'a> {
    pub fn new(key: &str, path: &'a str) -> Self {
        match key.split_once(':').map(|(kind, _)| kind) {
            Some(wall_proto::kind::WE) => Self::Workshop,
            Some(wall_proto::kind::STATIC | wall_proto::kind::VIDEO) if !path.is_empty() => {
                Self::Folder(Path::new(path))
            }
            _ => Self::Other,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LibraryProfile {
    pub id: String,
    pub name: String,
    folders: Vec<(PathBuf, FolderState)>,
    workshop: FolderState,
}

#[derive(Deserialize)]
struct Entry {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    show: Vec<String>,
    #[serde(default)]
    hide: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
    #[serde(default)]
    workshop: FolderState,
}

impl LibraryProfile {
    pub fn from_value(entry: &Value) -> Option<Self> {
        let entry = Entry::deserialize(entry).ok()?;
        (!entry.id.is_empty() && entry.id != EVERYTHING_PROFILE).then(|| Self::from_entry(entry))
    }

    fn from_entry(entry: Entry) -> Self {
        let mut folders = Vec::new();
        for (paths, state) in [
            (entry.show, FolderState::Show),
            (entry.hide, FolderState::Hide),
            (entry.exclude, FolderState::Exclude),
        ] {
            folders.extend(
                paths
                    .iter()
                    .filter(|path| !path.trim().is_empty())
                    .map(|path| PathBuf::from(crate::resolve(path)))
                    .filter(|path| path.is_absolute())
                    .filter_map(|path| crate::resolve_folder(&path))
                    .map(|path| (path, state)),
            );
        }
        folders.sort_by_key(|(path, _)| std::cmp::Reverse(path.components().count()));
        Self { id: entry.id, name: entry.name, folders, workshop: entry.workshop }
    }

    pub fn state(&self, item: LibraryItem<'_>) -> FolderState {
        let path = match item {
            LibraryItem::Folder(path) => path,
            LibraryItem::Workshop => return self.workshop,
            LibraryItem::Other => return FolderState::Show,
        };
        let Some(path) = crate::resolve_folder(path) else {
            return FolderState::Show;
        };
        self.folders
            .iter()
            .find(|(folder, _)| path.starts_with(folder))
            .map_or(FolderState::Show, |(_, state)| *state)
    }
}

pub fn library_profiles(root: &Value) -> Vec<LibraryProfile> {
    crate::arr_ref(root, crate::keys::library::PROFILES)
        .iter()
        .filter_map(LibraryProfile::from_value)
        .collect()
}

pub fn library_profile(root: &Value, id: &str) -> Option<LibraryProfile> {
    library_profiles(root).into_iter().find(|profile| profile.id == id)
}

pub fn active_library_profile(root: &Value) -> Option<LibraryProfile> {
    library_profile(root, crate::str_ref(root, crate::keys::library::PROFILE, ""))
}

pub fn profile_state(profile: Option<&LibraryProfile>, item: LibraryItem<'_>) -> FolderState {
    profile.map_or(FolderState::Show, |profile| profile.state(item))
}

#[cfg(test)]
#[path = "profile_tests.rs"]
mod tests;
