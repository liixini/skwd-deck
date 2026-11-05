use serde::{Deserialize, Serialize};

pub const MAX_PREPARE_KEYS: usize = 128;
pub const STATIC_KEY_PREFIX: &str = "static:";
pub const VIDEO_KEY_PREFIX: &str = "video:";
pub const WE_KEY_PREFIX: &str = "we:";
pub const TASK: &str = "drawing";

pub mod arg {
    pub const PACK: &str = "--pack";
    pub const CHECK: &str = "--check";
    pub const INPUT: &str = "--input";
    pub const OUTPUT: &str = "--output";
    pub const SOURCE_HASH: &str = "--source-sha256";
    pub const DEVICE: &str = "--device";
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub enum Device {
    Cpu,
    Cuda,
    Webgpu,
    #[default]
    Auto,
}

const DEVICE_NAMES: [(Device, &str); 4] = [
    (Device::Cpu, "cpu"),
    (Device::Cuda, "cuda"),
    (Device::Webgpu, "webgpu"),
    (Device::Auto, "auto"),
];

impl Device {
    pub const fn label(self) -> &'static str {
        DEVICE_NAMES[self as usize].1
    }

    pub fn parse(label: &str) -> Option<Self> {
        DEVICE_NAMES.iter().find(|(_, name)| *name == label).map(|(device, _)| *device)
    }
}

impl TryFrom<String> for Device {
    type Error = String;

    fn try_from(label: String) -> Result<Self, String> {
        Self::parse(&label).ok_or_else(|| format!("unknown device {label}"))
    }
}

pub mod stage {
    pub const CHECKING: &str = "checking";
    pub const STARTING: &str = "starting";
    pub const SOURCE: &str = "source";
    pub const LINEART: &str = "lineart";
    pub const DEPTH: &str = "depth";
    pub const SUBJECTS: &str = "subjects";
    pub const COMPLETE: &str = "complete";
    pub const READY: &str = "ready";
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    pub stage: String,
    #[serde(default)]
    pub progress: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackState {
    #[default]
    Missing,
    Available,
    Invalid,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    #[default]
    Idle,
    Checking,
    Preparing,
    Paused,
    Ready,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Status {
    pub pack: PackState,
    pub pack_path: String,
    pub pack_bytes: u64,
    pub key: String,
    pub active_key: String,
    pub phase: Phase,
    pub stage: String,
    pub progress: f32,
    pub prepared: bool,
    #[serde(default)]
    pub enabled: bool,
    pub preview_path: Option<String>,
    pub cache_bytes: u64,
    pub elapsed_ms: u64,
    pub completed: u32,
    pub total: u32,
    #[serde(default)]
    pub failed: u32,
    pub error: Option<String>,
}

impl Status {
    pub fn pack_missing(&self) -> bool {
        self.pack == PackState::Missing && self.phase != Phase::Checking
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct KeyRequest {
    #[serde(default)]
    pub key: String,
    pub output: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub path: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PrepareRequest {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default)]
    pub library: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SetEnabledRequest {
    pub key: String,
    pub enabled: bool,
}

#[cfg(test)]
#[path = "drawing_tests.rs"]
mod tests;
