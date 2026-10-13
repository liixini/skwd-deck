use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::state::WallState;
use crate::{db, paths};

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "bmp", "gif", "tiff", "tif"];

fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extensions.contains(&extension.to_ascii_lowercase().as_str()))
}

pub(super) fn is_image(path: &Path) -> bool {
    has_extension(path, IMAGE_EXTENSIONS)
}

pub(super) fn is_video(path: &Path) -> bool {
    paths::is_video_path(path)
}

pub(super) fn mtime_secs(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_secs() as i64)
}

pub(super) fn known_mtimes(state: &WallState) -> HashMap<String, i64> {
    state.with_db(db::known_keys).map(|rows| rows.into_iter().collect()).unwrap_or_default()
}

fn blocks_ready(thumb: &Path) -> bool {
    let (near, far) = crate::blocks::dests_for(&thumb.to_string_lossy());
    near.exists() && far.exists()
}

pub(super) fn artifacts_ready(thumb: &Path) -> bool {
    thumb.exists() && blocks_ready(thumb)
}

pub(super) fn candidates(
    state: &WallState,
    accepts: fn(&Path) -> bool,
) -> Vec<(PathBuf, String, i64)> {
    let config = state.config().clone();
    let policy = config.library_policy();
    let mut seen = std::collections::HashSet::new();
    config
        .library_folders()
        .into_iter()
        .flat_map(|directory| {
            WalkDir::new(directory)
                .follow_links(false)
                .into_iter()
                .filter_entry(|entry| {
                    !paths::is_internal_library_path(entry.path()) && !policy.excludes(entry.path())
                })
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_file() && accepts(entry.path()))
                .map(walkdir::DirEntry::into_path)
                .collect::<Vec<_>>()
        })
        .filter(|path| seen.insert(path.clone()))
        .filter_map(|path| {
            let key = state.library_key(&path)?;
            let (_, name) = key.split_once(':')?;
            let mtime = mtime_secs(&path);
            Some((path, name.to_string(), mtime))
        })
        .collect()
}

pub(super) fn we_sources_unchanged(item: &Path, video: Option<&Path>, thumb: &Path) -> bool {
    let modified = |path: &Path| std::fs::metadata(path).and_then(|meta| meta.modified()).ok();
    let Some(generated) = modified(thumb) else { return false };
    let preview = crate::we::find_preview(item);
    [Some(item), Some(item.join("project.json").as_path()), preview.as_deref(), video]
        .into_iter()
        .flatten()
        .all(|source| modified(source).is_some_and(|modified| modified <= generated))
}
