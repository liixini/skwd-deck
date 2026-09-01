#![deny(unsafe_code)]

mod atomic;
mod environment;
mod getter_macro;
mod key_catalog;
mod power;
pub mod schema;
mod settings;
mod value;

pub use atomic::{atomic_write, atomic_write_mode};
pub use environment::{cache_dir, config_dir, config_path, env, home, resolve};
pub use key_catalog::keys;
pub use power::{
    DEFAULT_BATTERY_FPS, DEFAULT_BATTERY_VIDEO_IDLE_SECONDS, PowerSourceState, battery_fps,
    battery_percent, battery_percent_at, battery_saver_enabled, battery_video_idle_seconds,
    battery_wallpaper_performance, configured_gpu_preference, effective_gpu_preference,
    effective_picker_fps, effective_video_idle_seconds, effective_wallpaper_performance,
    on_battery_power, power_source_state, power_source_state_at, set_power_source_snapshot,
};
pub use settings::{
    cache_dir_of, canonicalize_paper_engine, canonicalize_we_renderer, locale, paper_engine,
    pexels_api_key, steam_enabled, theme_authority, theme_backend, theme_engine, theme_policy,
    unsplash_access_key, video_dir, video_preview_delay_ms, video_preview_enabled,
    wallhaven_enabled, wallpaper_dir, wallpaper_mute, wallpaper_volume,
};
pub use value::{
    arr_ref, bool_at, bool_false_unless_true, bool_true_unless_false, f64_ref, get, i64_ref,
    num_at, str_at, str_ref, u64_at, u64_ref,
};
