use std::collections::HashMap;

pub trait RendererSupervision: Send + Sync {
    fn signal_ready(&self, pid: u32);
    fn reap_exited(&self);
    fn set_paused(&self, paused: bool);
    fn set_session_paused(&self, session_id: u64, paused: bool);
    fn paused(&self) -> bool;
    fn assignments(&self) -> HashMap<String, String>;
    fn has_shared_video(&self) -> bool;
    fn send_audio(&self, outputs: Option<&[String]>, mute: Option<bool>, volume: Option<u32>);
    fn send_shared_video_audio(&self, mute: bool, volume: u32);
    fn send_multi_video_audio(&self, outputs: &[String], mute: bool, volume: u32);
    fn wallpaper_pids(&self) -> Vec<u32>;
    fn wallpaper_count(&self) -> usize;
    fn wallpaper_rss_mb(&self) -> u64;
    fn scene_pids(&self) -> Vec<u32>;
    fn scene_count(&self) -> usize;
    fn scene_rss_mb(&self) -> u64;
}
