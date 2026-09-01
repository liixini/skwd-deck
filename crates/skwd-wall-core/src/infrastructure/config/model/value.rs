use serde_json::Value;
use std::path::{Path, PathBuf};

use skwd_config::home;

#[cfg(test)]
use super::source::read_root;

mod display;
mod renderer;
mod theme;
mod transition;

pub use display::DisplayConfig;
pub use renderer::RendererConfig;
pub use theme::{Integration, ThemeConfig};
pub use transition::TransitionConfig;

#[derive(Clone)]
pub struct Config {
    pub(super) root: Value,
}

impl Config {
    fn root(&self) -> &Value {
        &self.root
    }

    pub fn display(&self) -> DisplayConfig<'_> {
        DisplayConfig::new(self)
    }

    pub fn renderer(&self) -> RendererConfig<'_> {
        RendererConfig::new(self)
    }

    pub fn theme(&self) -> ThemeConfig<'_> {
        ThemeConfig::new(self)
    }

    pub fn transition(&self) -> TransitionConfig<'_> {
        TransitionConfig::new(self)
    }

    pub fn image_trash_days(&self) -> u32 {
        self.get(skwd_config::keys::performance::IMAGE_TRASH_DAYS)
            .and_then(Value::as_f64)
            .map_or(7, |value| value.clamp(0.0, 3650.0) as u32)
    }

    pub fn max_thumb_jobs(&self) -> usize {
        self.get(skwd_config::keys::performance::MAX_THUMB_JOBS)
            .and_then(Value::as_f64)
            .map_or(16, |value| value.clamp(1.0, 32.0) as usize)
    }

    pub fn library_polling_interval_seconds(&self) -> u64 {
        self.get(skwd_config::keys::library::POLLING_INTERVAL_SECONDS)
            .and_then(Value::as_f64)
            .map_or(60, |value| value.clamp(15.0, 3600.0) as u64)
    }

    pub fn we_assets_dir(&self) -> String {
        let configured = self.str_at(skwd_config::keys::paths::STEAM_WE_ASSETS, "");
        if configured.is_empty() { configured } else { self.resolve(&configured) }
    }

    skwd_config::getters! {
        bing_enabled: bool(skwd_config::keys::sources::BING_ENABLED, false);
        history_enabled: on_unless_off(skwd_config::keys::history::ENABLED);
        library_polling_fallback: off_unless_on(skwd_config::keys::library::POLLING_FALLBACK);
        image_auto_delete_trash: bool(skwd_config::keys::performance::AUTO_DELETE_IMAGE_TRASH, false);
        image_auto_optimize: bool(skwd_config::keys::performance::AUTO_OPTIMIZE_IMAGES, false);
        image_optimize_preset: str(skwd_config::keys::performance::IMAGE_OPTIMIZE_PRESET, "balanced");
        image_optimize_resolution: str(skwd_config::keys::performance::IMAGE_OPTIMIZE_RESOLUTION, "2k");
        niri_backdrop_auto_theme: off_unless_on(skwd_config::keys::niri::BACKDROP_AUTO_THEME);
        niri_backdrop_blur_enabled: on_unless_off(skwd_config::keys::niri::OVERVIEW_BACKDROP_BLUR_ENABLED);
        niri_backdrop_follow_wallpaper: on_unless_off(skwd_config::keys::niri::BACKDROP_FOLLOW_WALLPAPER);
        niri_backdrop_theme: str(skwd_config::keys::niri::BACKDROP_THEME, "");
        niri_overview_backdrop: off_unless_on(skwd_config::keys::niri::OVERVIEW_BACKDROP);
        notify_on_change: on_unless_off(skwd_config::keys::general::NOTIFY_ON_WALLPAPER_CHANGE);
        pexels_enabled: bool(skwd_config::keys::sources::PEXELS_ENABLED, false);
        pick_only_mode: off_unless_on("pickOnlyMode");
        plasma_lock_screen_dynamic_raw: str(skwd_config::keys::plasma::LOCK_SCREEN_DYNAMIC, "poster");
        plasma_lock_screen_image_raw: str(skwd_config::keys::plasma::LOCK_SCREEN_IMAGE, "");
        plasma_lock_screen_mode_raw: str(skwd_config::keys::plasma::LOCK_SCREEN_MODE, "off");
        post_process_on_restore: off_unless_on("postProcessOnRestore");
        random_favourites_only: on_unless_off(skwd_config::keys::general::RANDOM_INCLUDE_FAVOURITES);
        random_rotate: off_unless_on(skwd_config::keys::general::RANDOM_ROTATE);
        restore_on_startup: on_unless_off("restoreOnStartup");
        schedule_apply_on_start: on_unless_off(skwd_config::keys::schedule::APPLY_ON_START);
        schedule_enabled: off_unless_on(skwd_config::keys::schedule::ENABLED);
        schedule_migrated: off_unless_on(skwd_config::keys::schedule::MIGRATED);
        semantic_index_profile: str(skwd_config::keys::semantic::INDEX_PROFILE, "full");
        semantic_manifest: str(skwd_config::keys::semantic::MANIFEST, "");
        steam_api_key: str(skwd_config::keys::steam::API_KEY, "");
        steam_username: str(skwd_config::keys::steam::USERNAME, "");
        unsplash_enabled: bool(skwd_config::keys::sources::UNSPLASH_ENABLED, false);
        vitals_enabled: on_unless_off(skwd_config::keys::vitals::ENABLED);
        wallhaven_api_key: str(skwd_config::keys::wallhaven::API_KEY, "");
        wallhaven_username: str(skwd_config::keys::wallhaven::USERNAME, "");
        workspace_enabled: off_unless_on(skwd_config::keys::workspace::ENABLED);
        youtube_enabled: bool(skwd_config::keys::sources::YOUTUBE_ENABLED, false);
    }

    pub fn from_root(root: Value) -> Self {
        Self { root }
    }

    #[must_use]
    pub fn with_override(&self, path: &str, value: Value) -> Self {
        let mut next = self.clone();
        let parts = path.split('.').filter(|part| !part.is_empty()).collect::<Vec<_>>();
        let Some((last, parents)) = parts.split_last() else { return next };
        let mut current = &mut next.root;
        for part in parents {
            if !current.is_object() {
                *current = Value::Object(serde_json::Map::new());
            }
            current = current
                .as_object_mut()
                .expect("object established above")
                .entry((*part).to_string())
                .or_insert_with(|| Value::Object(serde_json::Map::new()));
        }
        if !current.is_object() {
            *current = Value::Object(serde_json::Map::new());
        }
        current
            .as_object_mut()
            .expect("object established above")
            .insert((*last).to_string(), value);
        next
    }

    fn get(&self, path: &str) -> Option<&Value> {
        skwd_config::get(&self.root, path)
    }

    fn str_at(&self, path: &str, default: &str) -> String {
        skwd_config::str_at(&self.root, path, default)
    }

    pub fn resolve(&self, path: &str) -> String {
        skwd_config::resolve(path)
    }

    pub fn wallpaper_dir(&self) -> String {
        skwd_config::wallpaper_dir(&self.root)
    }

    pub fn video_dir(&self) -> String {
        skwd_config::video_dir(&self.root)
    }

    pub fn cache_dir(&self) -> String {
        skwd_config::cache_dir_of(&self.root)
    }

    pub fn history_depth(&self) -> usize {
        self.get(skwd_config::keys::history::DEPTH)
            .and_then(Value::as_f64)
            .map_or(50, |num| num.clamp(1.0, 1000.0) as usize)
    }

    pub fn plasma_lock_screen_mode(&self) -> String {
        match self.plasma_lock_screen_mode_raw().trim().to_ascii_lowercase().as_str() {
            "static" => String::from("static"),
            "follow" => String::from("follow"),
            _ => String::from("off"),
        }
    }

    pub fn plasma_lock_screen_image(&self) -> String {
        let path = self.plasma_lock_screen_image_raw();
        if path.trim().is_empty() { String::new() } else { self.resolve(path.trim()) }
    }

    pub fn plasma_lock_screen_live(&self) -> bool {
        self.plasma_lock_screen_dynamic_raw().trim().eq_ignore_ascii_case("live")
    }

    pub fn video_preview_enabled(&self) -> bool {
        skwd_config::video_preview_enabled(&self.root)
    }

    pub fn video_preview_delay_ms(&self) -> u64 {
        skwd_config::video_preview_delay_ms(&self.root)
    }

    pub fn steam_enabled(&self) -> bool {
        skwd_config::steam_enabled(&self.root)
    }

    pub fn vitals_interval_mins(&self) -> u64 {
        let raw = skwd_config::num_at(&self.root, skwd_config::keys::vitals::INTERVAL_MINS, 10.0);
        (raw.max(1.0)) as u64
    }

    pub fn random_interval(&self) -> u64 {
        let raw = self
            .get(skwd_config::keys::general::RANDOM_INTERVAL)
            .and_then(Value::as_f64)
            .map_or(300, |val| val.max(0.0) as u64);
        if raw == 0 { 300 } else { raw.max(10) }
    }

    pub fn locale(&self) -> String {
        skwd_config::locale(&self.root)
    }

    pub fn latitude(&self) -> f64 {
        skwd_config::num_at(&self.root, skwd_config::keys::schedule::LATITUDE, 0.0)
    }

    pub fn longitude(&self) -> f64 {
        skwd_config::num_at(&self.root, skwd_config::keys::schedule::LONGITUDE, 0.0)
    }

    pub fn schedule_entries(&self) -> Vec<Value> {
        self.get(skwd_config::keys::schedule::ENTRIES)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    }

    pub fn schedule_rules(&self) -> Vec<Value> {
        self.get(skwd_config::keys::schedule::RULES)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    }

    pub fn schedule_solar(&self) -> bool {
        self.get(skwd_config::keys::schedule::TRIGGER).and_then(Value::as_str) != Some("fixed")
    }

    pub fn schedule_str(&self, field: &str) -> String {
        self.get(&format!("schedule.{field}")).and_then(Value::as_str).unwrap_or("").to_string()
    }

    pub fn workspace_wallpapers(&self) -> Vec<Value> {
        self.get(skwd_config::keys::workspace::WALLPAPERS)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    }

    pub fn workspace_debounce_ms(&self) -> u64 {
        self.get(skwd_config::keys::workspace::DEBOUNCE_MS).and_then(Value::as_u64).unwrap_or(60)
    }

    pub fn workspace_slide_ms(&self) -> u64 {
        self.get(skwd_config::keys::workspace::SLIDE_MS).and_then(Value::as_u64).unwrap_or(300)
    }

    pub fn playlist_lists(&self) -> Vec<Value> {
        self.get(skwd_config::keys::playlist::LISTS)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    }

    pub fn playlist_assign(&self) -> Vec<(String, String)> {
        self.get(skwd_config::keys::playlist::ASSIGN)
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|ent| {
                        let out = ent.get("output").and_then(Value::as_str)?;
                        let name = ent.get("playlist").and_then(Value::as_str)?;
                        (!out.is_empty() && !name.is_empty())
                            .then(|| (out.to_string(), name.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn random_types(&self) -> Vec<String> {
        let mut types = Vec::new();
        if skwd_config::schema::read_boolean(
            self.root(),
            skwd_config::keys::general::RANDOM_INCLUDE_STATIC,
        ) == Some(true)
        {
            types.push(wall_proto::kind::STATIC.to_string());
        }
        if skwd_config::schema::read_boolean(
            self.root(),
            skwd_config::keys::general::RANDOM_INCLUDE_VIDEO,
        ) == Some(true)
        {
            types.push(wall_proto::kind::VIDEO.to_string());
        }
        if skwd_config::schema::read_boolean(
            self.root(),
            skwd_config::keys::general::RANDOM_INCLUDE_WE,
        ) == Some(true)
        {
            types.push(wall_proto::kind::WE.to_string());
        }
        types
    }

    pub fn we_dir(&self) -> PathBuf {
        let ws = self.str_at(skwd_config::keys::paths::STEAM_WORKSHOP, "");
        if !ws.is_empty() {
            return PathBuf::from(self.resolve(&ws));
        }
        self.steam_dir().join("steamapps/workshop/content/431960")
    }

    pub fn niri_backdrop_blur(&self) -> f32 {
        self.get(skwd_config::keys::niri::OVERVIEW_BACKDROP_BLUR)
            .and_then(Value::as_f64)
            .map_or(20.0, |val| val.clamp(0.0, 200.0) as f32)
    }

    pub fn niri_backdrop_dim(&self) -> u32 {
        self.get(skwd_config::keys::niri::BACKDROP_DIM)
            .and_then(Value::as_f64)
            .map_or(0, |val| val.clamp(0.0, 100.0) as u32)
    }

    pub fn niri_backdrop_source(&self) -> String {
        self.resolve(&self.str_at(skwd_config::keys::niri::BACKDROP, ""))
    }

    pub fn post_processing(&self) -> Vec<(String, String)> {
        self.get("postProcessing")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|ent| match ent {
                        Value::String(cmd) => Some((cmd.clone(), String::from("all"))),
                        Value::Object(obj) => {
                            let cmd = obj.get("command").and_then(Value::as_str)?.to_string();
                            let ty = obj
                                .get("type")
                                .and_then(Value::as_str)
                                .unwrap_or("all")
                                .to_string();
                            Some((cmd, ty))
                        }
                        _ => None,
                    })
                    .filter(|(cmd, _)| !cmd.trim().is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn wallhaven_enabled(&self) -> bool {
        skwd_config::wallhaven_enabled(&self.root)
    }

    pub fn source_enabled(&self, source: &str) -> bool {
        match source {
            "wallhaven" => self.wallhaven_enabled(),
            "steam" => self.steam_enabled(),
            "bing" => self.bing_enabled(),
            "unsplash" => self.unsplash_enabled(),
            "pexels" => self.pexels_enabled(),
            "youtube" => self.youtube_enabled(),
            _ => false,
        }
    }

    pub fn bing_market(&self) -> String {
        let market = self.str_at(skwd_config::keys::sources::BING_MARKET, "en-US");
        if market.is_empty() { "en-US".to_string() } else { market }
    }

    pub fn unsplash_access_key(&self) -> String {
        skwd_config::unsplash_access_key(&self.root)
    }

    pub fn pexels_api_key(&self) -> String {
        skwd_config::pexels_api_key(&self.root)
    }

    pub fn youtube_max_height(&self) -> u32 {
        skwd_config::u64_at(&self.root, skwd_config::keys::sources::YOUTUBE_MAX_HEIGHT)
            .map_or(2160, |val| (val as u32).clamp(240, 4320))
    }

    pub fn youtube_max_minutes(&self) -> u64 {
        skwd_config::u64_at(&self.root, skwd_config::keys::sources::YOUTUBE_MAX_MINUTES)
            .unwrap_or(3)
            .min(600)
    }

    pub fn steam_backend(&self) -> String {
        let backend = self.str_at(skwd_config::keys::steam::BACKEND, "steam");
        if backend.is_empty() { "steam".to_string() } else { backend }
    }

    pub fn steam_dir(&self) -> PathBuf {
        let steam = self.str_at(skwd_config::keys::paths::STEAM, "");
        if steam.is_empty() {
            detect_steam_root(&home(), std::path::Path::is_dir)
        } else {
            PathBuf::from(self.resolve(&steam))
        }
    }

    pub fn steam_install_root(&self) -> PathBuf {
        let we = self.we_dir();
        if we.ends_with("steamapps/workshop/content/431960")
            && let Some(root) = we.ancestors().nth(4)
        {
            return root.to_path_buf();
        }
        self.steam_dir()
    }
}

pub fn detect_steam_root(home: &str, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let candidates = [
        ".local/share/Steam",
        ".steam/steam",
        ".steam/debian-installation",
        ".var/app/com.valvesoftware.Steam/.local/share/Steam",
        "snap/steam/common/.local/share/Steam",
    ];
    for cand in candidates {
        let path = Path::new(home).join(cand);
        if exists(&path.join("steamapps")) {
            return path;
        }
    }
    Path::new(home).join(".local/share/Steam")
}

#[path = "tests.rs"]
mod tests;
