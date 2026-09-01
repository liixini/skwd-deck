use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;

use crate::paths;
use crate::state::WallState;

use super::super::artifacts::{image_needs_thumb, scan_error_fresh};
use super::super::concurrency::scan_threads;
use super::common::{artifacts_ready, is_image, is_video, known_mtimes, mtime_secs};
use super::images::thumb_image;
use super::videos::thumb_video;

pub fn scan_paths<F>(state: &WallState, changed: &[PathBuf], on_item: F) -> usize
where
    F: Fn(&serde_json::Value) + Sync,
{
    let wallpaper_directory = PathBuf::from(state.config().wallpaper_dir());
    let video_directory = PathBuf::from(state.config().video_dir());
    let known = known_mtimes(state);
    let count = AtomicUsize::new(0);
    let work = || {
        changed.par_iter().for_each(|path| {
            if scan_path(state, &known, path, &wallpaper_directory, &video_directory, &on_item) {
                count.fetch_add(1, Ordering::Relaxed);
            }
        });
    };
    match rayon::ThreadPoolBuilder::new().num_threads(scan_threads()).build() {
        Ok(pool) => pool.install(work),
        Err(_) => work(),
    }
    count.load(Ordering::Relaxed)
}

fn scan_path<F>(
    state: &WallState,
    known: &HashMap<String, i64>,
    path: &Path,
    wallpaper_directory: &Path,
    video_directory: &Path,
    on_item: &F,
) -> bool
where
    F: Fn(&serde_json::Value),
{
    if !path.is_file() {
        return false;
    }
    let Some((kind, name)) = changed_kind(path, wallpaper_directory, video_directory) else {
        return false;
    };
    let mtime = mtime_secs(path);
    let key = format!("{kind}:{name}");
    let thumb = match kind {
        wall_proto::kind::VIDEO => paths::video_thumb(&name),
        _ => paths::image_thumb(&name),
    };
    let should_generate = match kind {
        wall_proto::kind::VIDEO => needs_regen(known, &key, mtime, artifacts_ready(&thumb)),
        _ => image_needs_thumb(
            known,
            &key,
            mtime,
            artifacts_ready(&thumb),
            scan_error_fresh(&thumb, mtime),
        ),
    };
    if !should_generate {
        return false;
    }

    match kind {
        wall_proto::kind::VIDEO => thumb_video(state, path, &name, mtime, on_item),
        _ => thumb_image(state, path, &name, mtime, on_item),
    }
}

pub(super) fn changed_kind(
    path: &Path,
    wallpaper_directory: &Path,
    video_directory: &Path,
) -> Option<(&'static str, String)> {
    if paths::is_internal_library_path(path) {
        return None;
    }
    if is_image(path) {
        let relative = path.strip_prefix(wallpaper_directory).ok()?;
        return Some((wall_proto::kind::STATIC, relative.to_string_lossy().into_owned()));
    }
    if is_video(path) {
        let relative = path
            .strip_prefix(video_directory)
            .or_else(|_| path.strip_prefix(wallpaper_directory))
            .ok()?;
        return Some((wall_proto::kind::VIDEO, relative.to_string_lossy().into_owned()));
    }
    None
}

pub(super) fn needs_regen(
    known: &HashMap<String, i64>,
    key: &str,
    mtime: i64,
    thumb_exists: bool,
) -> bool {
    known.get(key).copied() != Some(mtime) || !thumb_exists
}
