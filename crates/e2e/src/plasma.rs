use serde_json::{Map, Value};
use std::path::PathBuf;

use crate::sandbox::Sandbox;

pub const EVALUATE_SCRIPT: &str = "org.kde.PlasmaShell.evaluateScript";

pub fn script_assignments(script: &str) -> Option<Map<String, Value>> {
    let payload = script.strip_prefix("var a=")?.split_once(";var encoded=")?.0;
    serde_json::from_str(payload).ok()
}

pub struct FakePlasma {
    dir: PathBuf,
}

impl FakePlasma {
    pub fn install(sandbox: &mut Sandbox, qdbus: &str, kconfig: &str) -> Self {
        let dir = sandbox.root.join("plasma");
        let bin = dir.join("bin");
        let plugin = sandbox.root.join("data/plasma/wallpapers/org.skwd.wall.plasma");
        for path in [&bin, &plugin] {
            std::fs::create_dir_all(path).expect("fake Plasma dir");
        }
        std::fs::write(
            plugin.join("metadata.json"),
            r#"{"KPlugin":{"Id":"org.skwd.wall.plasma"}}"#,
        )
        .expect("plugin metadata");
        std::os::unix::fs::symlink(qdbus, bin.join("qdbus6")).expect("qdbus6 shim");
        for tool in ["kreadconfig6", "kwriteconfig6"] {
            std::os::unix::fs::symlink(kconfig, bin.join(tool)).expect("KConfig shim");
        }
        let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default());
        sandbox.set_env("PATH", &path);
        sandbox.set_env("XDG_CURRENT_DESKTOP", "KDE");
        sandbox.set_env("SKWD_E2E_PLASMA", &dir.to_string_lossy());
        Self { dir }
    }

    pub fn calls(&self) -> Vec<Vec<String>> {
        std::fs::read_to_string(self.dir.join("qdbus.log"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    pub fn scripts(&self) -> Vec<Map<String, Value>> {
        self.calls()
            .iter()
            .filter(|args| args.get(2).is_some_and(|method| method == EVALUATE_SCRIPT))
            .filter_map(|args| script_assignments(args.get(3)?))
            .collect()
    }

    pub fn fail_presentations(&self, message: &str) {
        std::fs::write(self.dir.join("failure"), message).expect("presentation failure");
    }

    pub fn confirm_presentations(&self) {
        let _ = std::fs::remove_file(self.dir.join("failure"));
    }
}
