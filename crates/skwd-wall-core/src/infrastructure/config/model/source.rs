use std::path::{Path, PathBuf};

use serde_json::Value;

use super::Config;

const CONFIG_MODE: u32 = 0o600;
const PERMISSION_BITS: u32 = 0o7777;

pub fn config_path() -> PathBuf {
    skwd_config::config_path()
}

pub(super) fn read_root(path: &Path) -> Value {
    Config::load_path_if_changed(path, &[]).map_or(Value::Null, |(config, _)| config.root)
}

fn secure_config(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        if let Ok(metadata) = std::fs::symlink_metadata(path)
            && metadata.file_type().is_file()
            && metadata.permissions().mode() & PERMISSION_BITS != CONFIG_MODE
        {
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(CONFIG_MODE));
        }
    }
}

impl Config {
    pub fn load() -> Self {
        Self { root: read_root(&config_path()) }
    }

    pub fn load_if_changed(previous: &[u8]) -> Option<(Self, Vec<u8>)> {
        Self::load_path_if_changed(&config_path(), previous)
    }

    pub(crate) fn load_path_if_changed(path: &Path, previous: &[u8]) -> Option<(Self, Vec<u8>)> {
        secure_config(path);
        let bytes = std::fs::read(path).ok()?;
        if bytes == previous {
            return None;
        }
        let mut root: Value = serde_json::from_slice(&bytes).ok()?;
        skwd_config::canonicalize_backdrop_blur(&mut root);
        Some((Self { root }, bytes))
    }
}
