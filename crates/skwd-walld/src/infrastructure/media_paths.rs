use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use skwd_wall_core::WallState;

const CONVERT_POLL_INTERVAL: Duration = Duration::from_millis(80);
const IDENTITY_KEY: &str = "key";
const STATIC_KEY_PREFIX: &str = "static:";

pub(crate) fn video_thumb(state: &Arc<WallState>, path: &str) -> Option<String> {
    state
        .with_db(|connection| skwd_wall_core::db::thumb_for_video(connection, path))
        .ok()
        .flatten()
        .filter(|thumb| Path::new(thumb).exists())
}

pub(crate) fn theme_image(state: &Arc<WallState>, image: &str) -> String {
    if image.is_empty() {
        return String::new();
    }
    let identity = skwd_wall_core::theme::profiles::identity(state, image);
    let Some(path) =
        identity[IDENTITY_KEY].as_str().and_then(|key| key.strip_prefix(STATIC_KEY_PREFIX))
    else {
        return image.to_string();
    };
    let config = state.config();
    let original = Path::new(&config.wallpaper_dir()).join(path);
    if original.is_file() && config.library_policy().contains(&original) {
        original.to_string_lossy().into_owned()
    } else {
        image.to_string()
    }
}

fn static_thumb(state: &Arc<WallState>, path: &str) -> Option<String> {
    let key = state.library_key(Path::new(path))?;
    state
        .with_db(|connection| skwd_wall_core::db::thumb_for_key(connection, &key))
        .ok()
        .flatten()
        .filter(|thumb| Path::new(thumb).exists())
}

pub(crate) fn studio_image(state: &Arc<WallState>, path: &str) -> String {
    video_thumb(state, path)
        .or_else(|| static_thumb(state, path))
        .unwrap_or_else(|| path.to_string())
}

pub(crate) fn output_theme_source(state: &Arc<WallState>, output: &str) -> Option<String> {
    let desired = skwd_wall_core::audio::read_state(&state.config().cache_dir());
    let entry = desired.get(output).or_else(|| desired.get("*"))?;
    let field = |key| entry.get(key).and_then(serde_json::Value::as_str).unwrap_or_default();
    let path = field("path");
    match field("type") {
        wall_proto::kind::STATIC if !path.is_empty() => Some(path.to_string()),
        wall_proto::kind::VIDEO => video_thumb(state, path),
        wall_proto::kind::WE if skwd_wall_core::we::valid_we_id(field("we_id")) => {
            skwd_wall_core::we::find_preview(&state.config().we_dir().join(field("we_id")))
                .map(|preview| preview.display().to_string())
        }
        _ => None,
    }
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
