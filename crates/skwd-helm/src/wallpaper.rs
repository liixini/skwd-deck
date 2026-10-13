use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

pub(super) fn path_for_key(key: &str, wallpaper_dir: &str, video_dir: &str) -> Option<String> {
    if let Some(relative) = key.strip_prefix("static:") {
        return Some(
            std::path::Path::new(wallpaper_dir).join(relative).to_string_lossy().into_owned(),
        );
    }
    if let Some(relative) = key.strip_prefix("video:") {
        return Some(std::path::Path::new(video_dir).join(relative).to_string_lossy().into_owned());
    }
    None
}

pub(super) fn kind_for_path(path: &str) -> &'static str {
    if paper_control::is_video_path(path) {
        wall_proto::kind::VIDEO
    } else {
        wall_proto::kind::STATIC
    }
}

pub(super) const LIST_ROWS: &str = "wallpapers";
pub(super) const LIST_COUNT: &str = "count";

pub(super) fn list_items(response: &Value) -> Vec<wall_proto::WallpaperItem> {
    response
        .get(LIST_ROWS)
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

pub(super) fn apply_params_for_item(
    item: &wall_proto::WallpaperItem,
    wallpaper_dir: &str,
    video_dir: &str,
    output: &str,
) -> Option<Value> {
    let kind = item.kind.as_deref().unwrap_or(wall_proto::kind::STATIC);
    if kind == wall_proto::kind::WE {
        let workshop_id = item.we_id.as_deref().filter(|id| !id.is_empty())?;
        return Some(
            json!({ "type": wall_proto::kind::WE, "we_id": workshop_id, "output": output }),
        );
    }
    let path = item
        .path
        .as_ref()
        .filter(|path| kind == wall_proto::kind::STATIC && !path.is_empty())
        .cloned()
        .or_else(|| {
            item.video_file
                .as_ref()
                .filter(|path| kind == wall_proto::kind::VIDEO && !path.is_empty())
                .cloned()
        })
        .or_else(|| path_for_key(item.key.as_deref()?, wallpaper_dir, video_dir))?;
    let kind = if kind == wall_proto::kind::VIDEO {
        wall_proto::kind::VIDEO
    } else {
        wall_proto::kind::STATIC
    };
    Some(json!({ "type": kind, "path": path, "output": output }))
}

pub(super) fn matches_filters(
    item: &wall_proto::WallpaperItem,
    kind: Option<&str>,
    tag: Option<&str>,
) -> bool {
    let kind_matches = kind.is_none_or(|wanted| item.kind.as_deref() == Some(wanted));
    let tag_matches = tag.is_none_or(|wanted| {
        item.tags
            .as_deref()
            .is_some_and(|tags| tags.split([',', ' ']).any(|tag| tag.trim() == wanted))
    });
    kind_matches && tag_matches
}

pub(super) fn shown(
    item: &wall_proto::WallpaperItem,
    profile: Option<&skwd_config::LibraryProfile>,
) -> bool {
    let key = item.key.as_deref().unwrap_or_default();
    let path = item.path.as_deref().or(item.video_file.as_deref()).unwrap_or_default();
    let state = skwd_config::profile_state(profile, skwd_config::LibraryItem::new(key, path));
    !item.hidden && state == skwd_config::FolderState::Show
}

pub(super) fn pick_index(length: usize, seed: u64) -> Option<usize> {
    (length > 0).then(|| (seed % length as u64) as usize)
}

pub(super) fn now_seed() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |duration| duration.as_nanos() as u64)
}
