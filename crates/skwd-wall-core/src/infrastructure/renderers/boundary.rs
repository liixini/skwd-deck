use std::collections::HashMap;

use crate::backend::renderers::RendererSupervision;

use super::RendererSupervisor;

impl RendererSupervision for RendererSupervisor {
    fn signal_ready(&self, pid: u32) {
        Self::signal_ready(self, pid);
    }

    fn reap_exited(&self) {
        self.reap_exited_paper();
    }

    fn set_paused(&self, paused: bool) {
        Self::set_paused(self, paused);
    }

    fn set_session_paused(&self, session_id: u64, paused: bool) {
        Self::set_session_paused(self, session_id, paused);
    }

    fn paused(&self) -> bool {
        Self::paused(self)
    }

    fn assignments(&self) -> HashMap<String, String> {
        Self::assignments(self)
    }

    fn has_shared_video(&self) -> bool {
        self.has_video_paper("*")
    }

    fn send_audio(&self, outputs: Option<&[String]>, mute: Option<bool>, volume: Option<u32>) {
        Self::send_audio(self, outputs, mute, volume);
    }

    fn send_shared_video_audio(&self, mute: bool, volume: u32) {
        Self::send_shared_video_audio(self, mute, volume);
    }

    fn send_multi_video_audio(&self, outputs: &[String], mute: bool, volume: u32) {
        Self::send_multi_video_audio(self, outputs, mute, volume);
    }

    fn wallpaper_pids(&self) -> Vec<u32> {
        Self::wallpaper_pids(self)
    }

    fn wallpaper_count(&self) -> usize {
        Self::wallpaper_count(self)
    }

    fn wallpaper_rss_mb(&self) -> u64 {
        Self::wallpaper_rss_mb(self)
    }

    fn scene_pids(&self) -> Vec<u32> {
        self.fleet_pids()
    }

    fn scene_count(&self) -> usize {
        self.fleet_len()
    }

    fn scene_rss_mb(&self) -> u64 {
        self.fleet_rss_mb()
    }
}
