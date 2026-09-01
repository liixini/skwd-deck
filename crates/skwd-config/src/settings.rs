use serde_json::Value;

use crate::{bool_true_unless_false, str_at};

pub fn wallpaper_dir(root: &Value) -> String {
    let dir = str_at(root, crate::keys::paths::WALLPAPER, "");
    if dir.is_empty() {
        format!("{}/Pictures/Wallpapers", crate::home())
    } else {
        crate::resolve(&dir)
    }
}

pub fn video_dir(root: &Value) -> String {
    let dir = str_at(root, crate::keys::paths::VIDEO_WALLPAPER, "");
    if dir.is_empty() { wallpaper_dir(root) } else { crate::resolve(&dir) }
}

pub fn cache_dir_of(root: &Value) -> String {
    let dir = str_at(root, crate::keys::paths::CACHE, "");
    if crate::env("SKWD_WALL_V2_CACHE").is_none() && !dir.is_empty() {
        crate::resolve(&dir)
    } else {
        crate::cache_dir()
    }
}

pub fn paper_engine(root: &Value) -> String {
    match str_at(root, crate::keys::paper::ENGINE, "").as_str() {
        "awww" => "awww".to_string(),
        _ => "skwd-paper".to_string(),
    }
}

pub fn canonicalize_paper_engine(root: &mut Value) -> bool {
    let Some(engine) = root
        .as_object_mut()
        .and_then(|object| object.get_mut("paper"))
        .and_then(Value::as_object_mut)
        .and_then(|paper| paper.get_mut("engine"))
    else {
        return false;
    };
    if matches!(engine.as_str(), Some("skwd-paper" | "awww")) {
        return false;
    }
    *engine = Value::String("skwd-paper".to_string());
    true
}

pub fn locale(root: &Value) -> String {
    str_at(root, crate::keys::general::LOCALE, "")
}

pub fn wallpaper_mute(root: &Value) -> bool {
    crate::schema::read_boolean(root, crate::keys::wallpaper::MUTE).unwrap_or(true)
}

pub fn wallpaper_volume(root: &Value) -> u32 {
    crate::schema::read_number(root, crate::keys::wallpaper::VOLUME)
        .map_or(100, |volume| volume.clamp(0.0, 100.0) as u32)
}

pub fn video_preview_enabled(root: &Value) -> bool {
    crate::schema::read_boolean(root, crate::keys::video_preview::ENABLED).unwrap_or(true)
}

pub fn video_preview_delay_ms(root: &Value) -> u64 {
    crate::schema::read_number(root, crate::keys::video_preview::DELAY_MS)
        .unwrap_or(250.0)
        .clamp(0.0, 3000.0) as u64
}

pub fn wallhaven_enabled(root: &Value) -> bool {
    bool_true_unless_false(root, crate::keys::features::WALLHAVEN)
}

pub fn steam_enabled(root: &Value) -> bool {
    bool_true_unless_false(root, crate::keys::features::STEAM)
}

pub fn canonicalize_we_renderer(root: &mut Value) -> bool {
    let Some(renderer) = root
        .as_object_mut()
        .and_then(|object| object.get_mut("weRender"))
        .and_then(Value::as_object_mut)
    else {
        return false;
    };
    let legacy = renderer.contains_key("native")
        || renderer.get("engine").is_some_and(|engine| engine.as_str() != Some("native"));
    if !legacy {
        return false;
    }
    renderer.remove("native");
    renderer.insert("engine".to_string(), Value::String("native".to_string()));
    true
}

pub fn unsplash_access_key(root: &Value) -> String {
    str_at(root, crate::keys::sources::UNSPLASH_ACCESS_KEY, "")
}

pub fn pexels_api_key(root: &Value) -> String {
    str_at(root, crate::keys::sources::PEXELS_API_KEY, "")
}

fn legacy_theme_backend(root: &Value) -> String {
    let backend = str_at(root, crate::keys::theme::BACKEND, "");
    if !backend.is_empty() {
        return backend;
    }
    "skwd-iris".to_string()
}

pub fn theme_policy(root: &Value) -> String {
    let policy = str_at(root, crate::keys::theme::POLICY, "");
    if matches!(policy.as_str(), "wallpaper" | "fixed" | "off") {
        return policy;
    }
    match legacy_theme_backend(root).as_str() {
        "static" => "fixed",
        "off" => "off",
        _ => "wallpaper",
    }
    .to_string()
}

pub fn theme_authority(root: &Value) -> String {
    let authority = str_at(root, crate::keys::theme::AUTHORITY, "");
    if matches!(authority.as_str(), "skwd" | "caelestia" | "noctalia" | "dms" | "end4") {
        return authority;
    }
    match legacy_theme_backend(root).as_str() {
        "noctalia" => "noctalia",
        "dms" => "dms",
        "caelestia" => "caelestia",
        "end4" => "end4",
        _ => "skwd",
    }
    .to_string()
}

pub fn theme_engine(root: &Value) -> String {
    let engine = str_at(root, crate::keys::theme::ENGINE, "");
    if !engine.is_empty() {
        return engine;
    }
    let legacy = legacy_theme_backend(root);
    if matches!(legacy.as_str(), "static" | "off" | "noctalia" | "dms") {
        "skwd-iris".to_string()
    } else {
        legacy
    }
}

pub fn theme_backend(root: &Value) -> String {
    match theme_policy(root).as_str() {
        "fixed" => "static".to_string(),
        "off" => "off".to_string(),
        _ => match theme_authority(root).as_str() {
            "noctalia" => "noctalia".to_string(),
            "dms" => "dms".to_string(),
            "caelestia" => "caelestia".to_string(),
            "end4" => "end4".to_string(),
            _ => theme_engine(root),
        },
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
