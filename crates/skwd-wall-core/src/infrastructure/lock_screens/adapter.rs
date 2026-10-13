use std::path::{Path, PathBuf};

use anyhow::{Result, ensure};
use serde_json::Value;
use skwd_config::keys::lock_screens as keys;
use wall_proto::LockScreenTarget;

use super::{ERROR_SEPARATOR, ownership, shells, text};

pub const DIRECTORY: &str = "lockscreens";
const CONFIG_HOME: &str = "XDG_CONFIG_HOME";
const CONFIG_DIRECTORY: &str = ".config";
const PATH_ENV: &str = "PATH";
const HYPRLOCK_BINARY: &str = "hyprlock";
const SWAYLOCK_BINARY: &str = "swaylock";
const DMS_BINARY: &str = "dms";
const NOCTALIA_BINARY: &str = "noctalia";
const QUICKSHELL_BINARY: &str = "quickshell";
const QS_BINARY: &str = "qs";

pub fn follows(config: &Value) -> bool {
    skwd_config::str_ref(config, keys::MODE, keys::OFF) == keys::FOLLOW
}

pub fn enabled(config: &Value) -> bool {
    matches!(skwd_config::str_ref(config, keys::MODE, keys::OFF), keys::FOLLOW | keys::STATIC)
}

pub fn requested(config: &Value) -> bool {
    enabled(config)
        && (keys::TARGET_FLAGS.iter().any(|key| skwd_config::bool_false_unless_true(config, key))
            || !skwd_config::str_ref(config, keys::SDDM_IMAGE, "").trim().is_empty())
}

pub fn config_home() -> PathBuf {
    std::env::var_os(CONFIG_HOME)
        .map_or_else(|| PathBuf::from(skwd_config::home()).join(CONFIG_DIRECTORY), PathBuf::from)
}

pub fn check_path(path: &Path) -> Result<()> {
    ensure!(path.is_absolute(), "Lockscreen path must be absolute");
    ensure!(!path.to_string_lossy().contains(['\n', '\r']), "Invalid lockscreen path");
    ownership::check(path)
}

fn binary(name: &'static str) -> bool {
    crate::paths::resolve_preferred_binary(None, std::env::var_os(PATH_ENV).as_deref(), &[name])
        .is_some()
}

pub fn target_available(target: LockScreenTarget) -> bool {
    match target {
        LockScreenTarget::Plasma => crate::infrastructure::plasma::available(),
        LockScreenTarget::Hyprlock => binary(HYPRLOCK_BINARY),
        LockScreenTarget::Swaylock => binary(SWAYLOCK_BINARY),
        LockScreenTarget::Dms => {
            binary(DMS_BINARY)
                || ((binary(QS_BINARY) || binary(QUICKSHELL_BINARY))
                    && crate::infrastructure::dms::shell_available(&config_home()))
        }
        LockScreenTarget::Noctalia => binary(NOCTALIA_BINARY),
        LockScreenTarget::Sddm => true,
    }
}

pub fn sync_target(
    config: &Value,
    image: Option<&Path>,
    root: &Path,
    target: LockScreenTarget,
) -> Result<()> {
    let home = config_home();
    match target {
        LockScreenTarget::Hyprlock => text::sync_hyprlock(config, image, root, &home),
        LockScreenTarget::Swaylock => text::sync_swaylock(config, image, root, &home),
        LockScreenTarget::Dms => shells::sync_dms(config, image, root),
        LockScreenTarget::Noctalia => shells::sync_noctalia(config, image, root),
        LockScreenTarget::Plasma | LockScreenTarget::Sddm => Ok(()),
    }
}

pub fn sync(config: &Value, image: Option<&Path>, root: &Path) -> Result<()> {
    let config_home = config_home();
    let mut errors = Vec::new();
    for result in [text::sync(config, image, root, &config_home), shells::sync(config, image, root)]
    {
        if let Err(error) = result {
            errors.push(error.to_string());
        }
    }
    ensure!(errors.is_empty(), "{}", errors.join(ERROR_SEPARATOR));
    Ok(())
}
