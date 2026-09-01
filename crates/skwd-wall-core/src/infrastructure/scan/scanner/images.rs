use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;
use walkdir::WalkDir;

use crate::state::WallState;
use crate::{db, media, paths};

use super::super::artifacts::{
    clear_scan_error, image_needs_thumb, scan_error_fresh, write_scan_error,
};
use super::super::catalog::{Row, row_item_json};
use super::super::concurrency::{decode_budget, image_decode_weight, scan_threads};
use super::common::{artifacts_ready, is_image, known_mtimes, mtime_secs};
use super::status::{disk_full, mark_disk_full};

pub fn scan<F>(state: &WallState, on_item: F) -> usize
where
    F: Fn(&serde_json::Value) + Sync,
{
    let directory = PathBuf::from(state.config().wallpaper_dir());
    if !directory.is_dir() {
        return 0;
    }

    let known = known_mtimes(state);
    let candidates: Vec<(PathBuf, String, i64)> = WalkDir::new(&directory)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !paths::is_internal_library_path(entry.path()))
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file() && is_image(entry.path()))
        .filter_map(|entry| {
            let relative =
                entry.path().strip_prefix(&directory).ok()?.to_string_lossy().into_owned();
            Some((entry.path().to_path_buf(), relative, mtime_secs(entry.path())))
        })
        .collect();

    let pending: Vec<&(PathBuf, String, i64)> = candidates
        .iter()
        .filter(|(_, name, mtime)| {
            let thumb = paths::image_thumb(name);
            image_needs_thumb(
                &known,
                &format!("static:{name}"),
                *mtime,
                artifacts_ready(&thumb),
                scan_error_fresh(&thumb, *mtime),
            )
        })
        .collect();

    log::debug!(
        "scan: dir={} candidates={} todo={} threads={}",
        directory.display(),
        candidates.len(),
        pending.len(),
        scan_threads()
    );
    let started = std::time::Instant::now();
    let count = AtomicUsize::new(0);
    let work = || {
        pending.par_iter().for_each(|(path, name, mtime)| {
            if thumb_image(state, path, name, *mtime, &on_item) {
                count.fetch_add(1, Ordering::Relaxed);
            }
        });
    };

    match rayon::ThreadPoolBuilder::new().num_threads(scan_threads()).build() {
        Ok(pool) => pool.install(work),
        Err(_) => work(),
    }

    let generated = count.load(Ordering::Relaxed);
    if disk_full() {
        log::error!("scan: storage exhausted while writing thumbnails; import incomplete");
    }
    log::debug!("scan: generated {generated} thumbs in {} ms", started.elapsed().as_millis());
    generated
}

pub(super) fn thumb_image<F>(
    state: &WallState,
    path: &Path,
    name: &str,
    mtime: i64,
    on_item: &F,
) -> bool
where
    F: Fn(&serde_json::Value),
{
    let thumb = paths::image_thumb(name);
    let thumb_small = paths::image_thumb_sm(name);
    let result = {
        let _permit = decode_budget().acquire(image_decode_weight(path));
        match media::generate_image_thumbs(path, &thumb, &thumb_small) {
            Ok(result) => result,
            Err(error) => {
                if media::is_disk_full(&error) {
                    mark_disk_full();
                    log::error!("thumb gen hit storage exhaustion for {}: {error}", path.display());
                } else {
                    log::warn!("thumb gen failed for {}: {error}", path.display());
                    write_scan_error(&thumb, mtime);
                }
                return false;
            }
        }
    };
    clear_scan_error(&thumb);

    let filesize = std::fs::metadata(path).map_or(0, |metadata| metadata.len() as i64);
    let row = Row {
        key: format!("static:{name}"),
        name: name.to_string(),
        thumb: thumb.to_string_lossy().into_owned(),
        thumb_sm: thumb_small.to_string_lossy().into_owned(),
        mtime,
        hue: i64::from(media::hue_bucket(result.hue, result.sat)),
        sat: i64::from(result.sat),
        richness: i64::from(result.richness),
        filesize,
        width: i64::from(result.width),
        height: i64::from(result.height),
    };
    if state
        .with_db(|connection| {
            db::upsert_cache_entry(
                connection,
                &row.key,
                wall_proto::kind::STATIC,
                &row.name,
                &row.thumb,
                &row.thumb_sm,
                "",
                "",
                row.mtime,
                row.hue,
                row.sat,
                row.richness,
                row.filesize,
                row.width,
                row.height,
            )
        })
        .is_err()
    {
        return false;
    }
    on_item(&row_item_json(&row));
    true
}
