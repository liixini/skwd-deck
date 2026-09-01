use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;

use super::Config;

pub fn config_path() -> PathBuf {
    skwd_config::config_path()
}

pub(super) fn read_root(path: &Path) -> Value {
    try_read_root(path).unwrap_or(Value::Null)
}

fn try_read_root(path: &Path) -> Option<Value> {
    #[cfg(unix)]
    if std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file()) {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok())
}

impl Config {
    pub fn load() -> Self {
        Self { root: read_root(&config_path()) }
    }

    pub fn current_mtime() -> Option<SystemTime> {
        std::fs::metadata(config_path()).and_then(|metadata| metadata.modified()).ok()
    }

    pub fn load_if_changed(previous: Option<SystemTime>) -> Option<(Self, SystemTime)> {
        Self::load_valid_path_if_changed(&config_path(), previous)
    }

    #[cfg(test)]
    pub(super) fn load_path_if_changed(
        path: &Path,
        previous: Option<SystemTime>,
    ) -> Option<(Self, SystemTime)> {
        let modified = std::fs::metadata(path).and_then(|metadata| metadata.modified()).ok()?;
        if previous == Some(modified) {
            return None;
        }
        Some((Self { root: read_root(path) }, modified))
    }

    pub(crate) fn load_valid_path_if_changed(
        path: &Path,
        previous: Option<SystemTime>,
    ) -> Option<(Self, SystemTime)> {
        let modified = std::fs::metadata(path).and_then(|metadata| metadata.modified()).ok()?;
        if previous == Some(modified) {
            return None;
        }
        Some((Self { root: try_read_root(path)? }, modified))
    }
}
