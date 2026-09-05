use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;
use serde_json::json;

use crate::state::WallState;
use crate::{db, media, paths};

use super::super::concurrency::{decode_budget, image_decode_weight, scan_threads};
use super::common::{artifacts_ready, mtime_secs};

pub fn scan_we<F>(state: &WallState, on_item: F) -> usize
where
    F: Fn(&serde_json::Value) + Sync,
{
    let directory = state.config().we_dir();
    if !directory.is_dir() {
        return 0;
    }

    let known: HashMap<String, (i64, String, String)> = state
        .with_db(db::known_we_meta)
        .map(|rows| {
            rows.into_iter()
                .map(|(key, mtime, kind, video_file)| (key, (mtime, kind, video_file)))
                .collect()
        })
        .unwrap_or_default();

    let candidates: Vec<(PathBuf, String, i64)> = std::fs::read_dir(&directory)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir() && entry.path().join("project.json").is_file())
        .map(|entry| {
            let wallpaper_engine_id = entry.file_name().to_string_lossy().into_owned();
            let mtime = mtime_secs(&entry.path().join("project.json"));
            (entry.path(), wallpaper_engine_id, mtime)
        })
        .collect();

    let count = AtomicUsize::new(0);
    let work = || {
        candidates.par_iter().for_each(|(item_directory, wallpaper_engine_id, mtime)| {
            if scan_item(state, &known, item_directory, wallpaper_engine_id, *mtime, &on_item) {
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

fn generate_thumbnails(
    item_directory: &Path,
    wallpaper_engine_id: &str,
    video_path: Option<&Path>,
    thumb: &Path,
    thumb_small: &Path,
) -> Option<media::ThumbResult> {
    let result = if let Some(video_path) = video_path {
        let _permit = decode_budget().acquire_exclusive();
        media::generate_video_thumbs(video_path, thumb, thumb_small, 1).or_else(|error| {
            log::warn!("we video thumb failed for {wallpaper_engine_id}: {error}; trying preview");
            match crate::we::find_preview(item_directory) {
                Some(preview) => media::generate_image_thumbs(&preview, thumb, thumb_small),
                None => Err(error),
            }
        })
    } else {
        match crate::we::find_preview(item_directory) {
            Some(preview) => {
                let _permit = decode_budget().acquire(image_decode_weight(&preview));
                media::generate_image_thumbs(&preview, thumb, thumb_small)
            }
            None => Err(anyhow::anyhow!("project has no usable preview")),
        }
    };

    match result {
        Ok(result) => Some(result),
        Err(error) => {
            log::warn!("we thumb failed for {wallpaper_engine_id}: {error}; using placeholder");
            media::generate_placeholder_thumbs(thumb, thumb_small)
                .inspect_err(|error| {
                    log::warn!("we placeholder failed for {wallpaper_engine_id}: {error}");
                })
                .ok()
        }
    }
}

fn scan_item<F>(
    state: &WallState,
    known: &HashMap<String, (i64, String, String)>,
    item_directory: &Path,
    wallpaper_engine_id: &str,
    mtime: i64,
    on_item: &F,
) -> bool
where
    F: Fn(&serde_json::Value),
{
    let key = format!("we:{wallpaper_engine_id}");
    let (project_type, file) = crate::we::read_project_type(item_directory);
    if !crate::we::is_supported_project(&project_type) {
        let _ = state.with_db(|conn| db::delete_entries(conn, std::slice::from_ref(&key)));
        return false;
    }
    let is_video_project = project_type.eq_ignore_ascii_case("video");
    let video_path = is_video_project
        .then(|| crate::we::safe_item_join(item_directory, &file))
        .flatten()
        .filter(|path| path.is_file());
    if is_video_project && video_path.is_none() {
        let _ = state.with_db(|conn| db::delete_entries(conn, std::slice::from_ref(&key)));
        return false;
    }
    let is_video = video_path.is_some();
    let entry_type = if is_video { wall_proto::kind::VIDEO } else { wall_proto::kind::WE };
    let video_file = video_path.as_ref().map(|path| path.display().to_string()).unwrap_or_default();

    let thumb = paths::we_thumb(wallpaper_engine_id);
    let thumb_small = paths::we_thumb_sm(wallpaper_engine_id);
    let fresh = known.get(&key).is_some_and(|(known_mtime, known_type, known_video_file)| {
        *known_mtime == mtime && known_type == entry_type && *known_video_file == video_file
    }) && artifacts_ready(&thumb)
        && sources_unchanged(item_directory, video_path.as_deref(), &thumb);
    if fresh {
        return false;
    }

    let Some(result) = generate_thumbnails(
        item_directory,
        wallpaper_engine_id,
        video_path.as_deref(),
        &thumb,
        &thumb_small,
    ) else {
        return false;
    };
    let name = crate::we::read_project_title(item_directory)
        .unwrap_or_else(|| wallpaper_engine_id.to_string());
    let filesize = video_path
        .as_ref()
        .and_then(|path| std::fs::metadata(path).ok())
        .map_or(0, |metadata| metadata.len() as i64);
    let thumb = thumb.to_string_lossy().into_owned();
    let thumb_small = thumb_small.to_string_lossy().into_owned();
    let hue = i64::from(media::hue_bucket(result.hue, result.sat));
    let wallpaper_engine_id_column = if is_video { "" } else { wallpaper_engine_id };

    if state
        .with_db(|connection| {
            db::upsert_cache_entry(
                connection,
                &key,
                entry_type,
                &name,
                &thumb,
                &thumb_small,
                &video_file,
                wallpaper_engine_id_column,
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
        "type": entry_type,
        "thumb": thumb,
        "thumb_sm": thumb_small,
        "favourite": 0,
        "hue": hue,
        "sat": result.sat,
        "tags": serde_json::Value::Null,
        "colors": serde_json::Value::Null,
        "preview": if is_video {
            json!(paths::we_preview(wallpaper_engine_id).to_string_lossy())
        } else {
            serde_json::Value::Null
        },
        "video_file": if is_video {
            json!(video_file)
        } else {
            serde_json::Value::Null
        },
        "we_id": if is_video {
            serde_json::Value::Null
        } else {
            json!(wallpaper_engine_id)
        },
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

fn sources_unchanged(item: &Path, video: Option<&Path>, thumb: &Path) -> bool {
    let modified = |path: &Path| std::fs::metadata(path).and_then(|meta| meta.modified()).ok();
    let Some(generated) = modified(thumb) else { return false };
    let preview = crate::we::find_preview(item);
    [Some(item), Some(item.join("project.json").as_path()), preview.as_deref(), video]
        .into_iter()
        .flatten()
        .all(|source| modified(source).is_some_and(|modified| modified <= generated))
}

#[cfg(test)]
#[path = "wallpaper_engine_tests.rs"]
mod tests;
