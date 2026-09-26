use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use super::{catalogue::Recipe, manager::Environment};

pub(super) fn reload(env: &Environment, recipe: &Recipe) -> String {
    if !env.reload {
        return "configured".into();
    }
    if recipe.id == "fish" {
        return fish(env);
    }
    if recipe.id == "niri" {
        return "watching".into();
    }
    let Some(signal) = recipe.signal else { return "on-next-open".into() };
    let Ok(processes) = std::fs::read_dir("/proc") else { return "reload-needed".into() };
    let mut sent = false;
    let mut unverified = false;
    for process in processes.flatten() {
        let Ok(pid) = process.file_name().to_string_lossy().parse::<i32>() else { continue };
        if !process.metadata().is_ok_and(|meta| meta.uid() == unsafe { libc::geteuid() }) {
            continue;
        }
        let path = process.path();
        if !std::fs::read_to_string(path.join("comm")).is_ok_and(|name| name.trim() == recipe.id) {
            continue;
        }
        let process_config = process_config(&path);
        unverified |= process_config.is_none();
        if process_config.as_ref() != Some(&env.config) {
            continue;
        }
        if unsafe { libc::kill(pid, signal) } != 0 {
            return "reload-needed".into();
        }
        sent = true;
    }
    if sent {
        "reload-sent"
    } else if unverified {
        "reload-needed"
    } else {
        "on-next-open"
    }
    .into()
}

fn fish(env: &Environment) -> String {
    let notify = || -> Option<()> {
        let enabled = super::files::load(&env.receipts.join("fish.json")).ok().flatten()?.enabled;
        let mut child = crate::proc::tool(env.executable("fish")?)
            .args(["-c", "set -U __skwd_theme_revision $argv"])
            .arg(if enabled { "on" } else { "off" })
            .arg(crate::paths::tmp_suffix())
            .env("HOME", &env.home)
            .env("XDG_CONFIG_HOME", &env.config)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return status.success().then_some(()),
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
            }
        }
    };
    if notify().is_some() { "on-next-open" } else { "reload-needed" }.into()
}

pub(super) fn process_config(path: &Path) -> Option<PathBuf> {
    let root = path.parent()?;
    let mut current = path.to_path_buf();
    for _ in 0..4 {
        if !std::fs::metadata(&current).is_ok_and(|meta| meta.uid() == unsafe { libc::geteuid() }) {
            return None;
        }
        if let Ok(environment) = std::fs::read(current.join("environ")) {
            let variable = |key: &[u8]| {
                environment.split(|byte| *byte == 0).find_map(|entry| entry.strip_prefix(key))
            };
            return variable(b"XDG_CONFIG_HOME=")
                .filter(|value| !value.is_empty())
                .map(|value| PathBuf::from(String::from_utf8_lossy(value).as_ref()))
                .filter(|path| path.is_absolute())
                .or_else(|| {
                    variable(b"HOME=").map(|value| {
                        PathBuf::from(String::from_utf8_lossy(value).as_ref()).join(".config")
                    })
                });
        }
        let status = std::fs::read_to_string(current.join("status")).ok()?;
        let parent = status
            .lines()
            .find_map(|line| line.strip_prefix("PPid:"))
            .and_then(|id| id.trim().parse::<u32>().ok())?;
        if parent == 0 {
            return None;
        }
        current = root.join(parent.to_string());
    }
    None
}

#[cfg(test)]
#[path = "reload_tests.rs"]
mod tests;
