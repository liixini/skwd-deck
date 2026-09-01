use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::Value;

use super::model::{BaseWallpaper, WorkspaceRule};

pub(super) fn base_file(cache_dir: &str) -> PathBuf {
    PathBuf::from(cache_dir).join("workspace-base.json")
}

pub(super) fn base_to_json(base: &HashMap<String, BaseWallpaper>) -> Value {
    let mut map = serde_json::Map::new();
    for (output, entry) in base {
        map.insert(
            output.clone(),
            serde_json::json!({
                "type": entry.ty,
                "path": entry.path,
                "we_id": entry.we_id,
                "mute": entry.mute,
                "volume": entry.volume,
            }),
        );
    }
    Value::Object(map)
}

pub(super) fn base_from_json(value: &Value) -> HashMap<String, BaseWallpaper> {
    let mut base = HashMap::new();
    let Some(map) = value.as_object() else {
        return base;
    };
    for (output, entry) in map {
        let ty = entry.get("type").and_then(Value::as_str).unwrap_or("").to_string();
        let path = entry.get("path").and_then(Value::as_str).unwrap_or("").to_string();
        let we_id = entry.get("we_id").and_then(Value::as_str).unwrap_or("").to_string();
        if ty.is_empty() || (path.is_empty() && we_id.is_empty()) {
            continue;
        }
        base.insert(
            output.clone(),
            BaseWallpaper {
                ty,
                path,
                we_id,
                mute: entry.get("mute").and_then(Value::as_bool).unwrap_or(true),
                volume: entry.get("volume").and_then(Value::as_u64).unwrap_or(100) as u32,
            },
        );
    }
    base
}

pub(super) fn seed_last(
    outputs_state: &Value,
    rules: &[WorkspaceRule],
    wallpaper_dir: &str,
    video_dir: &str,
) -> HashMap<String, String> {
    let mut last = HashMap::new();
    let Some(map) = outputs_state.as_object() else {
        return last;
    };
    for (output, entry) in map {
        if output == "*" {
            continue;
        }
        let entry_kind = entry.get("type").and_then(Value::as_str).unwrap_or("");
        let entry_path = entry.get("path").and_then(Value::as_str).unwrap_or("");
        let entry_wallpaper_engine = entry.get("we_id").and_then(Value::as_str).unwrap_or("");
        for rule in rules {
            if !rule.output.is_empty() && rule.output != *output {
                continue;
            }
            let (kind, path, wallpaper_engine_id) = crate::composition::apply::key_apply_args(
                &rule.wallpaper,
                wallpaper_dir,
                video_dir,
            );
            let matches = kind == entry_kind
                && if kind == wall_proto::kind::WE {
                    wallpaper_engine_id == entry_wallpaper_engine
                } else {
                    path == entry_path
                };
            if matches {
                last.insert(output.clone(), rule.wallpaper.clone());
                break;
            }
        }
    }
    last
}
