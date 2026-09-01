use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::paths::target_bin;

pub struct Sandbox {
    pub root: PathBuf,
    pub keep_on_fail: bool,
    failed: bool,
    extra_env: Vec<(String, String)>,
}

impl Sandbox {
    pub fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("skwd-e2e-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for sub in ["runtime/skwd-wall-v2", "config/skwd-wall-v2", "cache", "data", "library"] {
            std::fs::create_dir_all(root.join(sub)).expect("sandbox dir");
        }
        set_mode(&root.join("runtime"), 0o700);
        set_mode(&root.join("runtime/skwd-wall-v2"), 0o700);
        Self { root, keep_on_fail: true, failed: false, extra_env: Vec::new() }
    }

    pub fn set_env(&mut self, key: &str, val: &str) {
        self.extra_env.push((key.to_string(), val.to_string()));
    }

    pub fn socket(&self) -> PathBuf {
        self.root.join("runtime/skwd-wall-v2/wall.sock")
    }

    pub fn config_path(&self) -> PathBuf {
        self.root.join("config/skwd-wall-v2/config.json")
    }

    pub fn library(&self) -> PathBuf {
        self.root.join("library")
    }

    pub fn sqlite_path(&self) -> PathBuf {
        self.root.join("data/skwd-wall-v2/wall.sqlite")
    }

    pub fn outputs_json(&self) -> Value {
        std::fs::read_to_string(self.root.join("cache/skwd-wall-v2/outputs.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(Value::Null)
    }

    pub fn last_wallpaper(&self) -> Value {
        std::fs::read_to_string(self.root.join("cache/skwd-wall-v2/last-wallpaper.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(Value::Null)
    }

    pub fn write_config(&self, cfg: &Value) {
        std::fs::write(self.config_path(), serde_json::to_string_pretty(cfg).expect("config json"))
            .expect("write config");
    }

    pub fn env(&self) -> Vec<(String, String)> {
        let dir = |sub: &str| self.root.join(sub).to_string_lossy().into_owned();
        vec![
            ("XDG_RUNTIME_DIR".into(), dir("runtime")),
            ("XDG_CONFIG_HOME".into(), dir("config")),
            ("XDG_CACHE_HOME".into(), dir("cache")),
            ("XDG_DATA_HOME".into(), dir("data")),
            ("SKWD_WALL_V2_SOCK".into(), self.socket().to_string_lossy().into_owned()),
            ("SKWD_WALLD_NO_REAP".into(), "1".into()),
            ("SKWD_WALL_LOG".into(), "info".into()),
        ]
        .into_iter()
        .chain(self.extra_env.iter().cloned())
        .collect()
    }

    pub fn walld_command(&self) -> Command {
        let mut cmd = Command::new(target_bin("skwd-walld"));
        for (key, val) in self.env() {
            cmd.env(key, val);
        }
        for stale in ["SKWD_WALL_CONFIG", "SKWD_WALL_V2_CONFIG", "SKWD_WALL_V2_CACHE"] {
            cmd.env_remove(stale);
        }
        for display in ["WAYLAND_DISPLAY", "WAYLAND_SOCKET", "DISPLAY"] {
            cmd.env_remove(display);
        }
        cmd
    }

    pub fn mark_failed(&mut self) {
        self.failed = true;
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        if (self.failed || std::thread::panicking()) && self.keep_on_fail {
            eprintln!("e2e: sandbox kept at {}", self.root.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
}
