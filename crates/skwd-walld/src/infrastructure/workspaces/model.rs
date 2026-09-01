use std::collections::HashMap;
use std::time::Instant;

pub(super) struct WorkspaceInfo {
    pub(super) output: String,
    pub(super) idx: u64,
    pub(super) name: Option<String>,
    pub(super) active: bool,
}

pub(super) struct WorkspaceRule {
    pub(super) output: String,
    pub(super) matcher: String,
    pub(super) wallpaper: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct BaseWallpaper {
    pub(super) ty: String,
    pub(super) path: String,
    pub(super) we_id: String,
    pub(super) mute: bool,
    pub(super) volume: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DesiredWallpaper {
    Pin(String),
    Base,
}

pub(super) struct WorkspaceRuntime {
    pub(super) topo: HashMap<u64, WorkspaceInfo>,
    pub(super) pending: HashMap<String, DesiredWallpaper>,
    pub(super) last: HashMap<String, String>,
    pub(super) base: HashMap<String, BaseWallpaper>,
    pub(super) dirs: HashMap<String, &'static str>,
    pub(super) deadline: Option<Instant>,
}
