use anyhow::{Context, Result, ensure};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use super::{Style, files, manager::Environment};

pub(super) fn argument(args: &[String], cwd: &Path) -> Result<Option<PathBuf>> {
    for (index, arg) in args.iter().enumerate().skip(1) {
        let value = if arg == "-s" || arg == "--style" {
            Some(args.get(index + 1).context("Waybar has no stylesheet after --style")?.as_str())
        } else {
            arg.strip_prefix("--style=")
                .or_else(|| arg.strip_prefix("-s").filter(|value| !value.is_empty()))
        };
        if let Some(value) = value {
            ensure!(!value.is_empty(), "Waybar has an empty stylesheet path");
            return Ok(Some(cwd.join(value)));
        }
    }
    Ok(None)
}

fn running(env: &Environment) -> Result<Vec<(PathBuf, bool)>> {
    if !env.reload {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in std::fs::read_dir("/proc")?.flatten() {
        if entry.file_name().to_string_lossy().parse::<u32>().is_err()
            || !entry.metadata().is_ok_and(|meta| meta.uid() == unsafe { libc::geteuid() })
        {
            continue;
        }
        let process = entry.path();
        if !std::fs::read_to_string(process.join("comm")).is_ok_and(|name| name.trim() == "waybar")
            || super::super::reload::process_config(&process).as_ref() != Some(&env.config)
        {
            continue;
        }
        let bytes = std::fs::read(process.join("cmdline"))?;
        let args: Vec<_> = bytes
            .split(|byte| *byte == 0)
            .filter(|arg| !arg.is_empty())
            .map(|arg| String::from_utf8_lossy(arg).into_owned())
            .collect();
        let cwd = std::fs::read_link(process.join("cwd"))?;
        if let Some(path) = argument(&args, &cwd)? {
            paths.push((path, false));
        } else {
            paths.extend(defaults(env)?);
        }
    }
    Ok(paths)
}

fn defaults(env: &Environment) -> Result<Vec<(PathBuf, bool)>> {
    let directories: Vec<_> =
        [env.config.join("waybar"), env.home.join(".config/waybar"), env.home.join("waybar")]
            .into_iter()
            .map(|path| (path, false))
            .chain(env.config_dirs.iter().map(|path| (path.join("waybar"), true)))
            .collect();
    let paths: Vec<_> = ["style.css", "style-light.css", "style-dark.css"]
        .into_iter()
        .filter_map(|name| {
            directories
                .iter()
                .map(|(directory, shared)| (directory.join(name), *shared))
                .find(|(path, _)| path.is_file())
        })
        .collect();
    ensure!(
        !paths.is_empty(),
        "Waybar has no stylesheet; install its default style.css or start Waybar with --style"
    );
    Ok(paths)
}

pub(super) fn discover(env: &Environment) -> Result<Vec<Style>> {
    let mut paths = running(env)?;
    if paths.is_empty() {
        paths = defaults(env)?;
    }
    let mut styles = Vec::new();
    for (path, shared) in paths {
        let target = if shared {
            env.config
                .join("waybar")
                .join(path.file_name().context("Waybar stylesheet has no file name")?)
        } else {
            std::fs::canonicalize(&path)?
        };
        if styles.iter().any(|style: &Style| style.path == target) {
            continue;
        }
        files::writable(&target)?;
        let original = files::read(&target)?;
        let base = if shared && original.is_none() {
            format!("@import \"{}\";\n", css_path(&path))
        } else {
            original.clone().context("Waybar stylesheet was removed")?
        };
        styles.push(Style { path: target, original, base });
    }
    Ok(styles)
}

pub(super) fn css_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\a ")
        .replace('\r', "\\d ")
}

pub(super) fn same(left: &Path, right: &Path) -> bool {
    fn resolved(path: &Path) -> PathBuf {
        if let Ok(path) = path.canonicalize() {
            return path;
        }
        let mut normalized = PathBuf::new();
        for part in path.components() {
            match part {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    normalized.pop();
                }
                other => normalized.push(other.as_os_str()),
            }
        }
        if let (Some(parent), Some(name)) = (normalized.parent(), normalized.file_name())
            && let Ok(parent) = parent.canonicalize()
        {
            return parent.join(name);
        }
        normalized
    }
    resolved(left) == resolved(right)
}
