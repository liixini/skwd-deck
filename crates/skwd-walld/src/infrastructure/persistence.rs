use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::json;
use skwd_wall_core::WallState;
use skwd_wall_core::backend::wallpaper::WallpaperApplication;

use crate::domain::history::HistoryEntry;

pub(crate) fn last_wallpaper_path() -> PathBuf {
    skwd_wall_core::paths::cache_dir().join("last-wallpaper.json")
}

pub(crate) fn forget_last_wallpaper_if(path: &str, we_id: &str) {
    let file = last_wallpaper_path();
    let Ok(text) = std::fs::read_to_string(&file) else {
        return;
    };
    let value = serde_json::from_str::<serde_json::Value>(&text).unwrap_or_default();
    let last_path = value.get("path").and_then(serde_json::Value::as_str).unwrap_or("");
    let last_we = value.get("we_id").and_then(serde_json::Value::as_str).unwrap_or("");
    let matches =
        (!path.is_empty() && last_path == path) || (!we_id.is_empty() && last_we == we_id);
    if matches {
        let _ = std::fs::remove_file(&file);
        log::info!("forgot last-wallpaper record for a deleted item");
    }
}

pub(crate) fn repoint_optimized_image(old_path: &str, new_path: &str) {
    let last = last_wallpaper_path();
    if let Ok(text) = std::fs::read_to_string(&last)
        && let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&text)
        && value.get("path").and_then(serde_json::Value::as_str) == Some(old_path)
    {
        value["path"] = json!(new_path);
        let _ = skwd_wall_core::paths::atomic_write(&last, value.to_string().as_bytes());
    }

    let cache = skwd_wall_core::paths::cache_dir().display().to_string();
    let mut state = skwd_wall_core::audio::read_state(&cache);
    let mut changed = false;
    if let Some(outputs) = state.as_object_mut() {
        for entry in outputs.values_mut() {
            if entry.get("path").and_then(serde_json::Value::as_str) == Some(old_path) {
                entry["path"] = json!(new_path);
                changed = true;
            }
        }
    }
    if changed {
        skwd_wall_core::audio::write_state(&cache, &state);
    }
}

pub(crate) fn current_entry() -> Option<HistoryEntry> {
    let text = std::fs::read_to_string(last_wallpaper_path()).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let kind = value.get("type").and_then(serde_json::Value::as_str)?;
    let path = value.get("path").and_then(serde_json::Value::as_str).unwrap_or("");
    let we_id = value.get("we_id").and_then(serde_json::Value::as_str).unwrap_or("");
    let mute = value.get("mute").and_then(serde_json::Value::as_bool).unwrap_or(true);
    let volume =
        value.get("volume").and_then(serde_json::Value::as_u64).map_or(0, |value| value as u32);
    let entry = HistoryEntry::new(kind, path, we_id, mute, volume);
    entry.is_valid().then_some(entry)
}

pub(crate) fn parse_last_source(serialized: &str) -> Option<(String, String)> {
    let value = serde_json::from_str::<serde_json::Value>(serialized).ok()?;
    let kind = value.get("type").and_then(serde_json::Value::as_str)?;
    if kind == wall_proto::kind::WE {
        let thumb = value
            .get("thumb")
            .and_then(serde_json::Value::as_str)
            .filter(|text| !text.is_empty())?;
        return Some((kind.to_string(), thumb.to_string()));
    }
    if kind != wall_proto::kind::STATIC && kind != wall_proto::kind::VIDEO {
        return None;
    }
    let path = value.get("path").and_then(serde_json::Value::as_str)?;
    if path.is_empty() {
        return None;
    }
    Some((kind.to_string(), path.to_string()))
}

pub(crate) fn last_matches_json(
    serialized: &str,
    kind: &str,
    path: &str,
    we_id: &str,
    mute: bool,
    volume: u32,
) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(serialized) else {
        return false;
    };
    value.get("type").and_then(serde_json::Value::as_str) == Some(kind)
        && value.get("path").and_then(serde_json::Value::as_str).unwrap_or("") == path
        && value.get("we_id").and_then(serde_json::Value::as_str).unwrap_or("") == we_id
        && value.get("mute").and_then(serde_json::Value::as_bool).unwrap_or(true) == mute
        && value.get("volume").and_then(serde_json::Value::as_u64).unwrap_or(0) == u64::from(volume)
}

pub(crate) fn last_matches(kind: &str, path: &str, we_id: &str, mute: bool, volume: u32) -> bool {
    match std::fs::read_to_string(last_wallpaper_path()) {
        Ok(text) => last_matches_json(&text, kind, path, we_id, mute, volume),
        Err(_) => false,
    }
}

pub(crate) fn last_any_source() -> Option<String> {
    let text = std::fs::read_to_string(last_wallpaper_path()).ok()?;
    let (_, path) = parse_last_source(&text)?;
    if !Path::new(&path).exists() {
        return None;
    }
    Some(path)
}

pub(crate) fn last_any_thumb() -> Option<String> {
    let text = std::fs::read_to_string(last_wallpaper_path()).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let kind = value.get("type").and_then(serde_json::Value::as_str)?;
    if kind != wall_proto::kind::STATIC
        && kind != wall_proto::kind::VIDEO
        && kind != wall_proto::kind::WE
    {
        return None;
    }
    let thumb =
        value.get("thumb").and_then(serde_json::Value::as_str).filter(|text| !text.is_empty());
    let path = if kind == wall_proto::kind::WE {
        None
    } else {
        value.get("path").and_then(serde_json::Value::as_str)
    };
    let source = thumb.or(path)?;
    Path::new(source).exists().then(|| source.to_string())
}

pub(crate) fn persist_last(
    kind: &str,
    path: &str,
    we_id: &str,
    mute: bool,
    volume: u32,
    source: &str,
) {
    let file = last_wallpaper_path();
    if let Some(directory) = file.parent() {
        std::fs::create_dir_all(directory).ok();
    }
    let value = json!({
        "type": kind,
        "path": path,
        "we_id": we_id,
        "mute": mute,
        "volume": volume,
        "thumb": source,
    });
    let _ = skwd_wall_core::paths::atomic_write(&file, value.to_string().as_bytes());
}

pub(crate) fn reload_current_we(
    state: &Arc<WallState>,
    application: &dyn WallpaperApplication,
) -> anyhow::Result<bool> {
    if state.config().pick_only_mode() {
        return Ok(false);
    }
    let Ok(text) = std::fs::read_to_string(last_wallpaper_path()) else {
        return Ok(false);
    };
    let value: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    if value.get("type").and_then(serde_json::Value::as_str) != Some(wall_proto::kind::WE) {
        return Ok(false);
    }
    let Some(we_id) =
        value.get("we_id").and_then(serde_json::Value::as_str).filter(|text| !text.is_empty())
    else {
        return Ok(false);
    };
    if !skwd_wall_core::we::valid_we_id(we_id) {
        return Ok(false);
    }
    let item_dir = state.config().we_dir().join(we_id);
    if skwd_wall_core::we::read_project_type(&item_dir).0 != "scene" {
        return Ok(false);
    }
    application.apply_we(we_id)?;
    log::info!("wall.reload_we: re-applied WE scene {we_id} with current settings");
    Ok(true)
}

#[cfg(test)]
#[path = "persistence/tests.rs"]
mod tests;
