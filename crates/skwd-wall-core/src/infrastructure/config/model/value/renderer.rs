use serde_json::Value;

use super::Config;

fn colocated(name: &str) -> Option<String> {
    let executable = std::env::current_exe().ok()?;
    let candidate = executable.parent()?.join(name);
    if candidate.exists() { Some(candidate.display().to_string()) } else { None }
}

#[derive(Clone, Copy)]
pub struct RendererConfig<'a> {
    config: &'a Config,
}

impl<'a> RendererConfig<'a> {
    pub(super) fn new(config: &'a Config) -> Self {
        Self { config }
    }

    fn root(&self) -> &Value {
        self.config.root()
    }

    pub fn video_engine(&self) -> String {
        match self.config.str_at(skwd_config::keys::paper::VIDEO_ENGINE, "vulkan").as_str() {
            "tinier" => "tinier".to_string(),
            _ => "vulkan".to_string(),
        }
    }

    pub fn engine(&self) -> String {
        skwd_config::paper_engine(self.root())
    }

    skwd_config::getters! {
        awww_filter: str(skwd_config::keys::paper::AWWW_FILTER, "Lanczos3");
        we_disable_particles: bool(skwd_config::keys::we_render::DISABLE_PARTICLES, false);
        we_scaling: str(skwd_config::keys::we_render::SCALING, "default");
    }

    pub fn awww_arg(&self, key: &str) -> Option<String> {
        let value = self.config.get(&format!("paper.awww.{key}"))?;
        let text = match value {
            Value::String(text) => text.clone(),
            Value::Number(number) => match number.as_f64() {
                Some(value) if value.fract() == 0.0 => (value as i64).to_string(),
                _ => number.to_string(),
            },
            Value::Bool(flag) => flag.to_string(),
            _ => return None,
        };
        if text.is_empty() { None } else { Some(text) }
    }

    fn resolved_bin(self, environment_key: &str, config_path: &str, name: &str) -> String {
        if let Some(path) = skwd_config::env(environment_key) {
            return path;
        }
        let value = self.config.str_at(config_path, "");
        if !value.is_empty() {
            return self.config.resolve(&value);
        }
        colocated(name).unwrap_or_else(|| name.to_string())
    }

    pub fn still_bin(&self) -> String {
        self.resolved_bin(
            "SKWD_WALL_PAPER_STILL",
            skwd_config::keys::paths::PAPER_STILL_BIN,
            "skwd-wall-still",
        )
    }

    pub fn paper_bin(&self) -> String {
        if let Some(path) = skwd_config::env("SKWD_PAPER_BIN") {
            return path;
        }
        let configured = self.config.str_at(skwd_config::keys::paths::PAPER_BIN, "");
        if configured.is_empty() {
            crate::paths::paper_bin().display().to_string()
        } else {
            self.config.resolve(&configured)
        }
    }

    pub fn idle_pause_seconds(&self) -> u32 {
        let configured =
            skwd_config::u64_at(self.root(), skwd_config::keys::paper::IDLE_PAUSE_SECONDS)
                .unwrap_or(0)
                .min(u64::from(u32::MAX)) as u32;
        skwd_config::effective_video_idle_seconds(
            self.root(),
            skwd_config::on_battery_power(),
            configured,
        )
    }

    pub fn video_multi_process(&self) -> bool {
        self.config
            .get(skwd_config::keys::paper::VIDEO_MULTI_PROCESS)
            .and_then(Value::as_bool)
            .unwrap_or(true)
    }

    pub fn performance_mode(&self) -> bool {
        let configured = self
            .config
            .get(skwd_config::keys::paper::PERFORMANCE_MODE)
            .and_then(Value::as_bool)
            .unwrap_or(false);
        skwd_config::effective_wallpaper_performance(
            self.root(),
            skwd_config::on_battery_power(),
            configured,
        )
    }

    pub fn vk_bin(&self) -> String {
        self.resolved_bin(
            "SKWD_WALL_PAPER_VK",
            skwd_config::keys::paths::PAPER_VK_BIN,
            "skwd-wall-vk",
        )
    }

    pub fn mute(&self) -> bool {
        skwd_config::wallpaper_mute(self.root())
    }

    pub fn volume(&self) -> u32 {
        skwd_config::wallpaper_volume(self.root())
    }

    pub fn we_fps(&self) -> u32 {
        skwd_config::schema::read_number(self.root(), skwd_config::keys::we_render::FPS)
            .unwrap_or(30.0) as u32
    }

    pub fn we_scene_fill_mode(&self) -> String {
        let scaling = self.we_scaling();
        if scaling.parse::<wall_geom::FillMode>().is_ok() {
            scaling
        } else {
            self.config.display().fill_mode()
        }
    }
}
