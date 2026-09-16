use super::Config;
use serde_json::Value;

#[derive(Clone, Copy)]
pub struct PlaybackConfig<'a> {
    config: &'a Config,
}
impl<'a> PlaybackConfig<'a> {
    pub(super) fn new(config: &'a Config) -> Self {
        Self { config }
    }
    fn root(&self) -> &Value {
        self.config.root()
    }
    skwd_config::getters! {
        full_width_pause: bool(skwd_config::keys::niri::FULL_WIDTH_PAUSE, false);
        overview_only: bool(skwd_config::keys::niri::OVERVIEW_ONLY_PLAYBACK, false);
        process_pause_enabled: bool(skwd_config::keys::playback::PROCESS_ENABLED, false);
        pause_processes: str(skwd_config::keys::playback::PROCESSES, "");
        fullscreen_pause: bool(skwd_config::keys::playback::FULLSCREEN, false);
        maximized_pause: bool(skwd_config::keys::playback::MAXIMIZED, false);
        fullscreen_scope: str(skwd_config::keys::playback::FULLSCREEN_SCOPE, "all");
        mute_on_other_audio: bool(skwd_config::keys::playback::MUTE_ON_OTHER_AUDIO, false);
    }
    pub fn window_pause_enabled(&self) -> bool {
        self.fullscreen_pause() || self.maximized_pause() || self.full_width_pause()
    }

    pub fn resume_delay(&self) -> std::time::Duration {
        let seconds =
            skwd_config::num_at(self.root(), skwd_config::keys::playback::RESUME_DELAY, 1.0);
        std::time::Duration::from_secs_f64(if seconds.is_finite() {
            seconds.clamp(0.0, 60.0)
        } else {
            1.0
        })
    }
}
