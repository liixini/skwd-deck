use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;
use walkdir::WalkDir;

use crate::lock;
use crate::state::WallState;
use crate::{db, media, paths};

use super::super::concurrency::{decode_budget, scan_threads};
use super::common::{artifacts_ready, is_video, known_mtimes, mtime_secs};

static VIDEO_DECODE_GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn scan_videos<F>(state: &WallState, on_item: F) -> usize
where
    F: Fn(&serde_json::Value) + Sync,
{
    let mut directories = Vec::new();
    for directory in
        [PathBuf::from(state.config().video_dir()), PathBuf::from(state.config().wallpaper_dir())]
    {
        if directory.is_dir() && !directories.iter().any(|seen| seen == &directory) {
            directories.push(directory);
        }
    }
    if directories.is_empty() {
        return 0;
    }

    let known = known_mtimes(state);
    let candidates: Vec<(PathBuf, String, i64)> = directories
        .iter()
        .flat_map(|directory| {
            WalkDir::new(directory)
                .follow_links(false)
                .into_iter()
                .filter_entry(|entry| !paths::is_internal_library_path(entry.path()))
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_file() && is_video(entry.path()))
                .filter_map(|entry| {
                    let relative =
                        entry.path().strip_prefix(directory).ok()?.to_string_lossy().into_owned();
                    Some((entry.path().to_path_buf(), relative, mtime_secs(entry.path())))
                })
                .collect::<Vec<_>>()
        })
        .collect();

    let pending: Vec<&(PathBuf, String, i64)> = candidates
        .iter()
        .filter(|(_, name, mtime)| {
            let unchanged = known.get(&format!("video:{name}")).copied() == Some(*mtime);
            !unchanged || !artifacts_ready(&paths::video_thumb(name))
        })
        .collect();

    let count = AtomicUsize::new(0);
    let work = || {
        pending.par_iter().for_each(|(path, name, mtime)| {
            if thumb_video(state, path, name, *mtime, &on_item) {
                count.fetch_add(1, Ordering::Relaxed);
            }
        });
    };

    match rayon::ThreadPoolBuilder::new().num_threads(scan_threads().min(2)).build() {
        Ok(pool) => pool.install(work),
        Err(_) => work(),
    }
    count.load(Ordering::Relaxed)
}

pub(super) fn thumb_video<F>(
    state: &WallState,
    path: &Path,
    name: &str,
    mtime: i64,
    on_item: &F,
) -> bool
where
    F: Fn(&serde_json::Value),
{
    let thumb = paths::video_thumb(name);
    let thumb_small = paths::video_thumb_sm(name);
    let result = {
        let _gate = lock(&VIDEO_DECODE_GATE);
        let _permit = decode_budget().acquire_exclusive();
        match media::generate_video_thumbs(path, &thumb, &thumb_small, 1) {
            Ok(result) => result,
            Err(error) => {
                log::warn!("video thumb failed for {}: {error}", path.display());
                return false;
            }
        }
    };

    let filesize = std::fs::metadata(path).map_or(0, |metadata| metadata.len() as i64);
    let key = format!("video:{name}");
    let video_file = path.display().to_string();
    let thumb = thumb.to_string_lossy().into_owned();
    let thumb_small = thumb_small.to_string_lossy().into_owned();
    let hue = i64::from(media::hue_bucket(result.hue, result.sat));
    if state
        .with_db(|connection| {
            db::upsert_cache_entry(
                connection,
                &key,
                wall_proto::kind::VIDEO,
                name,
                &thumb,
                &thumb_small,
                &video_file,
                "",
                mtime,
                hue,
                i64::from(result.sat),
                i64::from(result.richness),
                filesize,
                i64::from(result.width),
                i64::from(result.height),
            )?;
            db::update_duration(connection, &key, result.duration_ms)?;
            Ok(())
        })
        .is_err()
    {
        return false;
    }

    on_item(&serde_json::json!({
        "key": key,
        "name": name,
        "type": wall_proto::kind::VIDEO,
        "thumb": thumb,
        "thumb_sm": thumb_small,
        "favourite": 0,
        "hue": hue,
        "sat": result.sat,
        "tags": serde_json::Value::Null,
        "colors": serde_json::Value::Null,
        "preview": paths::video_preview(name).to_string_lossy(),
        "video_file": video_file,
        "we_id": serde_json::Value::Null,
        "filesize": filesize,
        "width": result.width,
        "height": result.height,
        "duration_ms": result.duration_ms,
        "mtime": mtime,
        "richness": result.richness,
        "apply_count": 0,
        "last_applied": 0,
    }));
    true
}
