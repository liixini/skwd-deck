use std::collections::HashMap;
use std::path::Path;

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
