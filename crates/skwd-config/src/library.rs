use std::path::{Component, Path, PathBuf};

use serde_json::Value;

pub fn excluded_folders(root: &Value) -> Vec<String> {
    crate::schema::setting::library::EXCLUDED_FOLDERS.read(root)
}

pub fn library_folders(root: &Value) -> Vec<String> {
    let configured = crate::get(root, crate::keys::library::FOLDERS)
        .filter(|value| value.is_array())
        .map_or_else(
            || vec![crate::wallpaper_dir(root), crate::video_dir(root)],
            |_| crate::schema::setting::library::FOLDERS.read(root),
        );
    let mut folders = Vec::new();
    for folder in configured.into_iter().filter(|folder| !folder.trim().is_empty()) {
        let Some(path) = resolve_folder(Path::new(&crate::resolve(&folder))) else {
            continue;
        };
        let path = path.to_string_lossy().into_owned();
        if !folders.contains(&path) {
            folders.push(path);
        }
    }
    folders
}

pub fn library_destination(root: &Value, video: bool) -> Option<String> {
    let folders = library_folders(root);
    let legacy = if video { crate::video_dir(root) } else { crate::wallpaper_dir(root) };
    let policy = LibraryPolicy::new(root);
    let legacy_path = Path::new(&legacy);
    if policy.contains(legacy_path) && !policy.excludes(legacy_path) {
        return Some(legacy);
    }
    folders.into_iter().find(|folder| !policy.excludes(Path::new(folder)))
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LibraryPolicy {
    folders: Vec<PathBuf>,
    #[serde(default)]
    sources: Option<Vec<PathBuf>>,
}

impl LibraryPolicy {
    pub fn new(root: &Value) -> Self {
        let sources: Vec<PathBuf> = library_folders(root).into_iter().map(PathBuf::from).collect();
        let mut folders = Vec::new();
        for folder in excluded_folders(root).into_iter().filter(|folder| !folder.trim().is_empty())
        {
            let path = PathBuf::from(crate::resolve(&folder));
            if path.is_absolute() {
                folders.push(normalize(&path));
            } else {
                folders.extend(sources.iter().map(|source| normalize(&source.join(&path))));
            }
        }
        Self { folders, sources: Some(sources) }
    }

    pub fn contains(&self, path: &Path) -> bool {
        let Some(path) = resolve_folder(path) else {
            return false;
        };
        self.sources
            .as_ref()
            .is_none_or(|sources| sources.iter().any(|source| path.starts_with(source)))
    }

    pub fn allows_file(&self, path: &Path) -> bool {
        self.contains(path)
            && !self.excludes(path)
            && path.is_file()
            && !path.ancestors().any(Path::is_symlink)
    }

    pub fn excludes(&self, path: &Path) -> bool {
        if self.folders.is_empty() {
            return false;
        }
        let Some(path) = resolve_folder(path) else {
            return false;
        };
        self.folders.iter().any(|folder| path.starts_with(folder))
    }
}

pub fn resolve_folder(path: &Path) -> Option<PathBuf> {
    std::path::absolute(path).ok().map(|path| normalize(&path))
}

fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

#[cfg(test)]
#[path = "library_tests.rs"]
mod tests;
