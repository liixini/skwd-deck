#[derive(Clone, Copy)]
pub struct ApplyStaticRequest<'a> {
    pub output: &'a str,
    pub path: &'a str,
    pub fill_mode: &'a str,
    pub from: Option<&'a str>,
    pub transition: bool,
    pub shader: &'a str,
    pub duration_ms: u64,
}

#[derive(Clone, Copy)]
pub struct ApplyVideoRequest<'a> {
    pub output: &'a str,
    pub path: &'a str,
    pub fill_mode: &'a str,
    pub mute: bool,
    pub volume: u32,
}

#[derive(Clone, Copy)]
pub struct StaticSmartRequest<'a> {
    pub output: &'a str,
    pub path: &'a str,
    pub fill_mode: &'a str,
}

#[derive(Clone, Copy)]
pub struct ApplyOutputRequest<'a> {
    pub output: &'a str,
    pub kind: &'a str,
    pub path: &'a str,
    pub we_id: &'a str,
    pub fill_mode: &'a str,
    pub mute: bool,
    pub volume: u32,
    pub frame_rate: Option<&'a str>,
    pub transition: Option<OutputTransitionRequest<'a>>,
}

#[derive(Clone, Copy)]
pub struct OutputTransitionRequest<'a> {
    pub enabled: bool,
    pub shader: &'a str,
    pub duration_ms: u64,
}

#[derive(Clone, Copy)]
pub struct VideoTransitionRequest<'a> {
    pub from: &'a str,
    pub to: &'a str,
    pub fill_mode: &'a str,
    pub shader: &'a str,
    pub duration_ms: u64,
    pub mute: bool,
    pub volume: u32,
}

pub trait WallpaperApplication: Send + Sync {
    fn apply_static(&self, request: ApplyStaticRequest<'_>) -> anyhow::Result<()>;
    fn apply_static_smart(&self, request: StaticSmartRequest<'_>) -> anyhow::Result<()>;
    fn apply_video(&self, request: ApplyVideoRequest<'_>) -> anyhow::Result<()>;
    fn apply_output(&self, request: ApplyOutputRequest<'_>) -> anyhow::Result<()>;
    fn apply_video_transition(&self, request: VideoTransitionRequest<'_>) -> anyhow::Result<()>;
    fn apply_we(&self, we_id: &str) -> anyhow::Result<Option<String>>;
    fn video_engine_is_vk(&self) -> bool;
    fn reload_we(&self) -> anyhow::Result<()>;
}
