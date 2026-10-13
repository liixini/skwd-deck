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
    let config = state.config().clone();
    let known = known_mtimes(state);
    let count = AtomicUsize::new(0);
    let work = || {
        changed.par_iter().for_each(|path| {
            if scan_path(state, &known, path, &config, &on_item) {
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
    config: &crate::config::Config,
    on_item: &F,
) -> bool
where
    F: Fn(&serde_json::Value),
{
    if !state.config().library_policy().allows_file(path) {
        return false;
    }
    let Some((kind, mut name)) = changed_kind(path, config) else {
        return false;
    };
    if kind == wall_proto::kind::VIDEO {
        let Some(key) = state.library_key(path) else {
            return false;
        };
        let Some((_, stored)) = key.split_once(':') else {
            return false;
        };
        name = stored.to_string();
    }
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
    config: &crate::config::Config,
) -> Option<(&'static str, String)> {
    if !is_image(path) && !is_video(path) {
        return None;
    }
    let key = paths::key_for_path(path, config)?;
    let (_, name) = key.split_once(':')?;
    let kind = if is_video(path) { wall_proto::kind::VIDEO } else { wall_proto::kind::STATIC };
    Some((kind, name.to_string()))
}

pub(super) fn needs_regen(
    known: &HashMap<String, i64>,
    key: &str,
    mtime: i64,
    thumb_exists: bool,
) -> bool {
    known.get(key).copied() != Some(mtime) || !thumb_exists
}
