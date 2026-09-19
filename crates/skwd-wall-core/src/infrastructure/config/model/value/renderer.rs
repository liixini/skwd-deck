use serde_json::Value;

use super::Config;

mod binaries;

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

    pub fn load_timeout(&self) -> std::time::Duration {
        let seconds = skwd_config::schema::read_number(
            self.root(),
            skwd_config::keys::paper::LOAD_TIMEOUT_SECONDS,
        )
        .unwrap_or(3.0);
        std::time::Duration::from_secs_f64(seconds)
    }

    pub fn gpu_device(&self) -> String {
        skwd_config::configured_gpu_device(self.root())
    }

    pub fn wallpaper_layer(&self) -> String {
        match self.config.str_at(skwd_config::keys::paper::WALLPAPER_LAYER, "bottom").as_str() {
            "background" => "background",
            "top" => "top",
            "overlay" => "overlay",
            _ => "bottom",
        }
        .to_string()
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
        we_clamp: str(skwd_config::keys::we_render::CLAMP, "clamp");
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
