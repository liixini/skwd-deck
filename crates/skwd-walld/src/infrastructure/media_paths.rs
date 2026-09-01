use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use skwd_wall_core::WallState;
use skwd_wall_core::db::{TINIER_CONVERT_MAX_BYTES, TINIER_CONVERT_PRESET};

const CONVERT_POLL_INTERVAL: Duration = Duration::from_millis(80);

pub(crate) struct TinierVideo {
    pub(crate) path: String,
    pub(crate) frame_rate: String,
}

pub(crate) fn tinier_video(state: &Arc<WallState>, path: &str) -> Option<TinierVideo> {
    let (destination, frame_rate, preset, original_size) = state
        .with_db(|connection| skwd_wall_core::db::tinier_convert_entry(connection, path))
        .ok()
        .flatten()?;
    let metadata = std::fs::metadata(&destination).ok()?;
    let source_size = std::fs::metadata(path).ok()?.len() as i64;
    if preset != TINIER_CONVERT_PRESET
        || original_size != source_size
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > TINIER_CONVERT_MAX_BYTES
    {
        return None;
    }
    Some(TinierVideo { path: destination, frame_rate })
}

pub(crate) fn video_thumb(state: &Arc<WallState>, path: &str) -> Option<String> {
    state
        .with_db(|connection| skwd_wall_core::db::thumb_for_video(connection, path))
        .ok()
        .flatten()
        .filter(|thumb| Path::new(thumb).exists())
}

pub(crate) fn static_thumb(state: &Arc<WallState>, path: &str) -> Option<String> {
    let directory = state.config().wallpaper_dir();
    let relative =
        Path::new(path).strip_prefix(&directory).ok()?.to_string_lossy().replace('\\', "/");
    let key = format!("static:{relative}");
    state
        .with_db(|connection| skwd_wall_core::db::thumb_for_key(connection, &key))
        .ok()
        .flatten()
        .filter(|thumb| Path::new(thumb).exists())
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum VideoRoute<'a> {
    Transition(&'a str),
    Plain,
}

pub(crate) fn video_route<'a>(
    transitions_enabled: bool,
    from: Option<&'a str>,
    render_path: &str,
    thumb: Option<&'a str>,
) -> VideoRoute<'a> {
    if !transitions_enabled {
        return VideoRoute::Plain;
    }
    match from {
        Some(source) if source != render_path => VideoRoute::Transition(source),
        _ => thumb.map_or(VideoRoute::Plain, VideoRoute::Transition),
    }
}

pub(crate) fn converted_target(current: Option<&Path>, raw: &Path) -> Option<PathBuf> {
    match current {
        Some(path) if path != raw => Some(path.to_path_buf()),
        _ => None,
    }
}

pub(crate) fn await_converted_by(
    raw: &Path,
    timeout_ms: u64,
    find_current: impl Fn() -> Option<PathBuf>,
) -> String {
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(timeout_ms) {
        let current = find_current();
        if let Some(path) = converted_target(current.as_deref(), raw) {
            return path.to_string_lossy().into_owned();
        }
        std::thread::sleep(CONVERT_POLL_INTERVAL);
    }
    find_current().map_or_else(
        || raw.to_string_lossy().into_owned(),
        |path| path.to_string_lossy().into_owned(),
    )
}

pub(crate) fn await_converted(
    wallpaper_dir: &str,
    id: &str,
    raw: &Path,
    timeout_ms: u64,
) -> String {
    await_converted_by(raw, timeout_ms, || {
        crate::infrastructure::wallhaven::library_path(wallpaper_dir, id)
    })
}

#[cfg(test)]
#[path = "media_paths/tests.rs"]
mod tests;
