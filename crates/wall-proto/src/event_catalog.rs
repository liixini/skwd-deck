pub mod ev {
    use serde::{Deserialize, Serialize};

    pub const APPLIED: &str = "skwd.wall.applied";
    pub const APPLY_RESULT: &str = "skwd.wall.apply_result";
    pub const CACHED: &str = "skwd.wall.cached";
    pub const CLEARED: &str = "skwd.wall.cleared";
    pub const REMOVED: &str = "skwd.wall.removed";
    pub const FILE_REMOVED: &str = "skwd.wall.file_removed";
    pub const FILE_RENAMED: &str = "skwd.wall.file_renamed";
    pub const FOLDER_REMOVED: &str = "skwd.wall.folder_removed";
    pub const SCAN_DONE: &str = "skwd.wall.scan_done";
    pub const SEMANTIC_INDEX_READY: &str = "skwd.wall.semantic_index_ready";
    pub const CONFIG_CHANGED: &str = "skwd.wall.config_changed";
    pub const POWER_CHANGED: &str = "skwd.wall.power_changed";
    pub const OUTPUTS_CHANGED: &str = "skwd.wall.outputs_changed";
    pub const THEME_DONE: &str = "skwd.wall.theme_done";
    pub const DOWNLOAD: &str = "skwd.wall.download";
    pub const UNSUBSCRIBED: &str = "skwd.wall.unsubscribed";
    pub const PREVIEW_READY: &str = "skwd.wall.preview_ready";
    pub const REMOTE_THUMB: &str = "skwd.wall.remote_thumb";
    pub const WATCH_ERROR: &str = "skwd.wall.watch_error";
    pub const WATCH_STATUS: &str = "skwd.wall.watch_status";
    pub const HIDE: &str = "skwd.wall.hide";
    pub const TOGGLE: &str = "skwd.wall.toggle";
    pub const RECOMPUTE_PROGRESS: &str = "skwd.wall.recompute_colors.progress";
    pub const RECOMPUTE_COMPLETE: &str = "skwd.wall.recompute_colors.complete";
    pub const IMAGE_OPTIMIZE_COMPLETE: &str = "skwd.wall.image_optimize.complete";
    pub const TASK_STATUS: &str = "skwd.wall.task.status";

    fn default_true() -> bool {
        true
    }

    #[derive(Clone, Debug, Default, Serialize, Deserialize)]
    pub struct PreviewReady {
        pub id: String,
        pub path: String,
    }

    #[derive(Clone, Debug, Default, Serialize, Deserialize)]
    pub struct RemoteThumb {
        pub id: String,
        pub path: String,
    }

    #[derive(Clone, Debug, Default, Serialize, Deserialize)]
    pub struct Removed {
        pub key: String,
    }

    #[derive(Clone, Debug, Default, Serialize, Deserialize)]
    pub struct FileRemoved {
        pub name: String,
    }

    #[derive(Clone, Debug, Default, Serialize, Deserialize)]
    pub struct FileRenamed {
        #[serde(default)]
        pub old_name: String,
        #[serde(default)]
        pub new_name: String,
        #[serde(default)]
        pub filesize: u64,
        #[serde(default)]
        pub mtime: i64,
        #[serde(default)]
        pub width: u32,
        #[serde(default)]
        pub height: u32,
    }

    #[derive(Clone, Debug, Default, Serialize, Deserialize)]
    pub struct FolderRemoved {
        #[serde(default)]
        pub names: Vec<String>,
    }

    #[derive(Clone, Debug, Default, Serialize, Deserialize)]
    pub struct Unsubscribed {
        #[serde(default)]
        pub id: String,
        #[serde(default)]
        pub ok: bool,
        #[serde(default)]
        pub warn: bool,
    }

    #[derive(Clone, Debug, Default, Serialize, Deserialize)]
    pub struct PowerChanged {
        pub on_battery: bool,
        #[serde(default)]
        pub source: String,
    }

    #[derive(Clone, Debug, Default, Serialize, Deserialize)]
    pub struct OutputsChanged {
        #[serde(default)]
        pub outputs: Vec<String>,
    }

    #[derive(Clone, Debug, Default, Serialize, Deserialize)]
    pub struct Applied {
        #[serde(default)]
        pub key: String,
        #[serde(rename = "type", default)]
        pub kind: String,
        #[serde(default)]
        pub random: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub path: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub we_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub mute: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub volume: Option<u64>,
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct ApplyResult {
        #[serde(default)]
        pub request_id: u64,
        #[serde(default = "default_true")]
        pub ok: bool,
        #[serde(default)]
        pub output: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub error_kind: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub detail: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub queued: Option<bool>,
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct ThemeDone {
        #[serde(default)]
        pub source: String,
        #[serde(default = "default_true")]
        pub ok: bool,
        #[serde(default)]
        pub backend: String,
        #[serde(default)]
        pub requested: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub external: Option<bool>,
    }

    impl Default for ThemeDone {
        fn default() -> Self {
            Self {
                source: String::new(),
                ok: true,
                backend: String::new(),
                requested: String::new(),
                external: None,
            }
        }
    }

    #[derive(Clone, Debug, Default, Serialize, Deserialize)]
    pub struct ScanDone {
        #[serde(default)]
        pub count: i64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub total: Option<i64>,
        #[serde(default)]
        pub disk_full: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub request_id: Option<String>,
    }

    #[derive(Clone, Debug, Default, Serialize, Deserialize)]
    pub struct SemanticIndexReady {
        #[serde(default)]
        pub items: u64,
        #[serde(default)]
        pub fingerprint: u64,
    }

    #[derive(Clone, Debug, Default, Serialize, Deserialize)]
    pub struct ConfigChanged {}

    #[derive(Clone, Debug, Default, Serialize, Deserialize)]
    pub struct WatchError {
        #[serde(default)]
        pub detail: String,
    }

    #[derive(Clone, Debug, Default, Serialize, Deserialize)]
    pub struct RecomputeProgress {
        #[serde(default)]
        pub progress: u64,
        #[serde(default)]
        pub total: u64,
    }

    #[derive(Clone, Debug, Default, Serialize, Deserialize)]
    pub struct RecomputeComplete {
        #[serde(default)]
        pub updated: u64,
        #[serde(default)]
        pub total: u64,
    }

    #[derive(Clone, Debug, Default, Serialize, Deserialize)]
    pub struct ImageOptimizeComplete {
        #[serde(default)]
        pub running: bool,
        #[serde(default)]
        pub candidates: u64,
        #[serde(default)]
        pub optimized: u64,
        #[serde(default)]
        pub skipped_quality: u64,
        #[serde(default)]
        pub skipped_savings: u64,
        #[serde(default)]
        pub skipped_other: u64,
        #[serde(default)]
        pub errors: u64,
        #[serde(default)]
        pub original_bytes: u64,
        #[serde(default)]
        pub optimized_bytes: u64,
        #[serde(default)]
        pub trash_deleted_files: u64,
        #[serde(default)]
        pub trash_deleted_bytes: u64,
        #[serde(default)]
        pub optimized_paths: Vec<String>,
    }
}

#[cfg(test)]
#[path = "event_catalog_tests.rs"]
mod tests;
