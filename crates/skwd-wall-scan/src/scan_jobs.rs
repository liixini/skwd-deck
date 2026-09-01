use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde_json::json;
use skwd_wall_core::{WallState, scan};
use wall_proto::rpc;

use crate::reporter::Reporter;

pub(crate) fn full_scan(state: &WallState, reporter: &Reporter, request_id: Option<&str>) {
    log::info!("scanning images in {}", state.config().wallpaper_dir());
    let image_count = scan::scan(state, |item| reporter.send(rpc::SCAN_ITEM, item));
    log::info!("scanning videos in {}", state.config().video_dir());
    let video_count = scan::scan_videos(state, |item| reporter.send(rpc::SCAN_ITEM, item));
    log::info!("scanning WE in {}", state.config().we_dir().display());
    let wallpaper_engine_count = scan::scan_we(state, |item| reporter.send(rpc::SCAN_ITEM, item));
    let pruned = prune(state, reporter);
    log::info!(
        "scan complete: {image_count} images, {video_count} videos, \
         {wallpaper_engine_count} we, {pruned} pruned"
    );
    reporter.send(
        rpc::SCAN_DONE,
        &scan_done_payload(
            image_count + video_count + wallpaper_engine_count,
            scan::take_disk_full(),
            None,
            request_id,
        ),
    );
}

pub(crate) fn changed_paths(
    state: &WallState,
    reporter: &Reporter,
    changed: &[PathBuf],
    request_id: Option<&str>,
) {
    let count = scan::scan_paths(state, changed, |item| reporter.send(rpc::SCAN_ITEM, item));
    log::info!("delta scan: {count} items from {} changed paths", changed.len());
    reporter.send(
        rpc::SCAN_DONE,
        &scan_done_payload(count, scan::take_disk_full(), Some(changed), request_id),
    );
}

pub(crate) fn scan_done_payload(
    count: usize,
    disk_full: bool,
    paths: Option<&[PathBuf]>,
    request_id: Option<&str>,
) -> serde_json::Value {
    let mut payload = json!({"count": count, "disk_full": disk_full});
    if let Some(paths) = paths {
        payload["paths"] = json!(paths);
    }
    if let Some(request_id) = request_id {
        payload["request_id"] = json!(request_id);
    }
    payload
}

pub(crate) fn recolor(state: &WallState, reporter: &Reporter) {
    use skwd_wall_core::{db, media};
    use std::sync::atomic::{AtomicUsize, Ordering};

    let rows = state.with_db(db::color_rows).unwrap_or_default();
    let total = rows.len();
    log::info!("recolor: {total} entries");
    reporter.send(rpc::RECOMPUTE_PROGRESS, &json!({ "progress": 0, "total": total }));
    let completed = AtomicUsize::new(0);
    let updated = AtomicUsize::new(0);
    rows.par_iter().for_each(|(key, thumb)| {
        if let Some((hue, saturation, richness)) = media::extract_colors_from(Path::new(thumb)) {
            let bucket = media::hue_bucket(hue, saturation);
            if state
                .with_db(|connection| {
                    db::update_colors(
                        connection,
                        key,
                        i64::from(bucket),
                        i64::from(saturation),
                        i64::from(richness),
                    )
                })
                .is_ok()
            {
                updated.fetch_add(1, Ordering::Relaxed);
            }
        }
        let seen = completed.fetch_add(1, Ordering::Relaxed) + 1;
        if seen.is_multiple_of(50) || seen == total {
            reporter.send(rpc::RECOMPUTE_PROGRESS, &json!({ "progress": seen, "total": total }));
        }
    });
    let updated = updated.load(Ordering::Relaxed);
    log::info!("recolor: re-extracted {updated}/{total}");
    reporter.send(rpc::RECOMPUTE_DONE, &json!({ "updated": updated, "total": total }));
}

pub(crate) fn key_present(
    key: &str,
    wallpaper_directory: &str,
    video_directory: &str,
    wallpaper_engine_directory: &Path,
) -> bool {
    if let Some(relative) = key.strip_prefix("static:") {
        Path::new(wallpaper_directory).join(relative).exists()
    } else if let Some(relative) = key.strip_prefix("video:") {
        Path::new(video_directory).join(relative).exists()
            || Path::new(wallpaper_directory).join(relative).exists()
    } else if let Some(workshop_id) = key.strip_prefix("we:") {
        wallpaper_engine_directory.join(workshop_id).join("project.json").is_file()
    } else {
        true
    }
}

pub(crate) fn prune_targets(key: &str) -> Vec<PathBuf> {
    use skwd_wall_core::{blocks, paths};

    let (thumbnail, small_thumbnail) = if let Some(relative) = key.strip_prefix("static:") {
        (paths::image_thumb(relative), paths::image_thumb_sm(relative))
    } else if let Some(relative) = key.strip_prefix("video:") {
        (paths::video_thumb(relative), paths::video_thumb_sm(relative))
    } else if let Some(workshop_id) = key.strip_prefix("we:") {
        (paths::we_thumb(workshop_id), paths::we_thumb_sm(workshop_id))
    } else {
        return Vec::new();
    };
    let (near, far) = blocks::dests_for(&thumbnail.to_string_lossy());
    vec![near, far, thumbnail, small_thumbnail]
}

fn prune(state: &WallState, reporter: &Reporter) -> usize {
    use skwd_wall_core::db;

    let keys: Vec<String> =
        state.with_db(db::known_keys).unwrap_or_default().into_iter().map(|(key, _)| key).collect();
    let (wallpaper_directory, video_directory, wallpaper_engine_directory) = {
        let config = state.config();
        (config.wallpaper_dir(), config.video_dir(), config.we_dir())
    };
    let mut gone = Vec::new();
    for key in keys {
        if key_present(&key, &wallpaper_directory, &video_directory, &wallpaper_engine_directory) {
            continue;
        }
        for target in prune_targets(&key) {
            let _ = std::fs::remove_file(target);
        }
        gone.push(key);
    }
    if !gone.is_empty() {
        let _ = state.with_db(|connection| db::delete_entries(connection, &gone));
        for key in &gone {
            reporter.send(rpc::SCAN_REMOVED, &json!({ "key": key }));
        }
    }
    gone.len()
}
