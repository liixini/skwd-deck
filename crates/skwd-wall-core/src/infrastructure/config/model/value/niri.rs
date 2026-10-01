use paper_control::SourceKind;
use serde_json::Value;
use skwd_config::keys::niri;

use super::Config;

impl Config {
    pub fn niri_stationary_active(&self) -> bool {
        self.niri_stationary_wallpaper() && std::env::var_os("NIRI_SOCKET").is_some()
    }

    pub fn niri_stationary_wallpaper(&self) -> bool {
        self.niri_overview_backdrop()
            && self.str_at(niri::OVERVIEW_MODE, "separate") == "stationary"
    }

    pub fn niri_backdrop_blur(&self, kind: SourceKind) -> u32 {
        let (enabled, radius) = match kind {
            SourceKind::Static => (niri::BACKDROP_BLUR_STATIC, niri::BACKDROP_BLUR_STATIC_RADIUS),
            SourceKind::Video => (niri::BACKDROP_BLUR_VIDEO, niri::BACKDROP_BLUR_VIDEO_RADIUS),
            SourceKind::WallpaperEngine => (niri::BACKDROP_BLUR_WE, niri::BACKDROP_BLUR_WE_RADIUS),
        };
        if !skwd_config::schema::read_boolean(self.root(), enabled).unwrap_or(false) {
            return 0;
        }
        skwd_config::schema::read_number(self.root(), radius).unwrap_or(0.0).clamp(0.0, 100.0)
            as u32
    }

    pub fn niri_backdrop_dim(&self) -> u32 {
        self.get(niri::BACKDROP_DIM)
            .and_then(Value::as_f64)
            .map_or(0, |val| val.clamp(0.0, 100.0) as u32)
    }

    pub fn niri_backdrop_source(&self) -> String {
        self.resolve(&self.str_at(niri::BACKDROP, ""))
    }
}
