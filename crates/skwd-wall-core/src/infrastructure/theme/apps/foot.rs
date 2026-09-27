use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::io::Write;
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, ensure};

use super::{catalogue::Recipe, files, manager::Environment};

pub(super) fn patch(
    env: &Environment,
    original: Option<&str>,
    output: &Path,
) -> Result<(String, String)> {
    let text = original.unwrap_or_default();
    ensure!(!text.contains("Skwd app theme"), "An existing Skwd setup needs review");
    let mut body = String::from("[main]\n");
    if original.is_none()
        && let Some(system) = env
            .config_dirs
            .iter()
            .map(|root| root.join("foot/foot.ini"))
            .find(|path| path.is_file())
    {
        body.push_str(&include(&system)?);
    }
    body.push_str(&include(output)?);
    let section = text
        .lines()
        .filter_map(|line| line.trim().strip_prefix('[')?.split_once(']').map(|(name, _)| name))
        .next_back()
        .unwrap_or("main");
    let _ = writeln!(body, "[{section}]");
    let separator = if text.is_empty() || text.ends_with('\n') { "" } else { "\n" };
    Ok((String::new(), format!("{separator}# Skwd app theme\n{body}# End Skwd app theme\n")))
}

pub(super) fn owns_include(line: &str, output: &Path) -> bool {
    line.split_once('=')
        .is_some_and(|(key, value)| key.trim() == "include" && Path::new(value.trim()) == output)
}

fn include(path: &Path) -> Result<String> {
    let path = path.to_str().context("Foot config path is not UTF-8")?;
    ensure!(!path.contains(['\n', '\r']), "Foot config path contains a line break");
    Ok(format!("include={path}\n"))
}

pub(super) fn validate(
    env: &Environment,
    path: &Path,
    text: &str,
    generated: Option<&str>,
) -> Result<()> {
    let parent = path.parent().context("Missing Foot config directory")?;
    std::fs::create_dir_all(parent)?;
    let mut replacement = None;
    let text = if let Some(generated) = generated {
        let mut file =
            tempfile::Builder::new().prefix(".skwd-colours-").suffix(".ini").tempfile_in(parent)?;
        file.write_all(generated.as_bytes())?;
        let next = text.replace(&include(&parent.join("skwd-colors.ini"))?, &include(file.path())?);
        replacement = Some(file);
        next
    } else {
        text.to_owned()
    };
    let mut staged =
        tempfile::Builder::new().prefix(".skwd-check-").suffix(".ini").tempfile_in(parent)?;
    staged.write_all(text.as_bytes())?;
    let result = Command::new(env.executable("foot").context("Foot is not installed")?)
        .args(["--check-config", "--config"])
        .arg(staged.path())
        .env("HOME", &env.home)
        .env("XDG_CONFIG_HOME", &env.config)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()?;
    ensure!(
        result.status.success(),
        "Foot rejected the config: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    drop(replacement);
    Ok(())
}

fn palette(text: &str) -> Option<Vec<u8>> {
    let mut dark = std::collections::BTreeMap::new();
    let mut light = std::collections::BTreeMap::new();
    let mut section = "";
    for line in text.lines().map(str::trim) {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|line| line.strip_suffix(']')) {
            section = name;
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let target = match section {
            "colors" | "colors-dark" => &mut dark,
            "colors-light" => &mut light,
            _ => continue,
        };
        target.insert(key.trim(), value.trim());
    }
    if dark != light || dark.len() != 20 {
        return None;
    }
    let mut commands = String::new();
    for (key, sequence) in [
        ("foreground", "10".to_owned()),
        ("background", "11".to_owned()),
        ("selection-background", "17".to_owned()),
        ("selection-foreground", "19".to_owned()),
    ]
    .into_iter()
    .map(|(key, sequence)| (key.to_owned(), sequence))
    .chain((0..16).map(|index| {
        (
            format!("{}{number}", if index < 8 { "regular" } else { "bright" }, number = index % 8),
            format!("4;{index}"),
        )
    })) {
        let value = dark.get(key.as_str())?;
        if value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let _ = write!(commands, "\x1b]{sequence};#{value}\x1b\\");
    }
    Some(commands.into_bytes())
}

fn ordinary(args: &[u8]) -> bool {
    let mut args = args.split(|byte| *byte == 0).skip(1);
    while let Some(argument) = args.next() {
        if argument.is_empty() || argument == b"--" || !argument.starts_with(b"-") {
            return true;
        }
        let name = argument.split(|byte| *byte == b'=').next().unwrap_or(argument);
        if matches!(
            name,
            b"--hold"
                | b"-H"
                | b"--maximized"
                | b"-m"
                | b"--fullscreen"
                | b"-F"
                | b"--login-shell"
                | b"-L"
                | b"--log-no-syslog"
        ) {
            continue;
        }
        if matches!(
            name,
            b"--app-id"
                | b"--class"
                | b"--working-directory"
                | b"-D"
                | b"-w"
                | b"-W"
                | b"--title"
                | b"--font"
                | b"--term"
                | b"--window-size-pixels"
                | b"--window-size-chars"
                | b"--log-level"
                | b"-a"
                | b"-T"
                | b"-f"
                | b"-t"
        ) {
            if !argument.contains(&b'=') && args.next().is_none_or(<[u8]>::is_empty) {
                return false;
            }
            continue;
        }
        return false;
    }
    true
}

fn eligible(env: &Environment, path: &Path) -> bool {
    let Some(expected) = env.executable("foot").and_then(|path| std::fs::metadata(path).ok())
    else {
        return false;
    };
    std::fs::metadata(path).is_ok_and(|meta| meta.uid() == unsafe { libc::geteuid() })
        && std::fs::metadata(path.join("exe"))
            .is_ok_and(|exe| exe.dev() == expected.dev() && exe.ino() == expected.ino())
        && super::reload::process_config(path).as_ref() == Some(&env.config)
        && std::fs::read(path.join("cmdline")).is_ok_and(|args| ordinary(&args))
}

fn terminals(env: &Environment, root: &Path) -> Vec<PathBuf> {
    let Ok(processes) = std::fs::read_dir(root) else { return Vec::new() };
    let mut terminals = BTreeSet::new();
    for process in processes.flatten() {
        let path = process.path();
        if !eligible(env, &path) {
            continue;
        }
        let Ok(descriptors) = std::fs::read_dir(path.join("fdinfo")) else { continue };
        for descriptor in descriptors.flatten() {
            let Ok(info) = std::fs::read_to_string(descriptor.path()) else { continue };
            if info.lines().any(|line| {
                line.strip_prefix("tty-index:")
                    .is_some_and(|index| index.trim().parse::<u32>().is_ok())
            }) {
                terminals.insert(descriptor.path());
            }
        }
    }
    terminals.into_iter().collect()
}

fn send(env: &Environment, descriptor: &Path, commands: &[u8]) -> Result<()> {
    let process = descriptor.parent().and_then(Path::parent).context("Missing Foot process")?;
    ensure!(eligible(env, process), "Foot process changed");
    let info = std::fs::read_to_string(descriptor)?;
    let index = info
        .lines()
        .find_map(|line| line.strip_prefix("tty-index:")?.trim().parse::<u32>().ok())
        .context("Foot terminal closed")?;
    let mut terminal = std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NOCTTY | libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(format!("/dev/pts/{index}"))?;
    let meta = terminal.metadata()?;
    ensure!(
        meta.file_type().is_char_device() && meta.uid() == unsafe { libc::geteuid() },
        "Foot terminal ownership changed"
    );
    ensure!(
        eligible(env, process) && std::fs::read_to_string(descriptor)? == info,
        "Foot terminal changed"
    );
    terminal.write_all(commands)?;
    Ok(())
}

pub(super) fn reload(env: &Environment, recipe: &Recipe) -> String {
    let (_, output, receipt) = env.paths(recipe);
    let enabled = files::load(&receipt).ok().flatten().is_some_and(|receipt| receipt.enabled);
    let commands = if enabled {
        let Some(commands) = files::read(&output).ok().flatten().and_then(|text| palette(&text))
        else {
            return "on-next-open".into();
        };
        commands
    } else {
        b"\x1b]104;0;1;2;3;4;5;6;7;8;9;10;11;12;13;14;15\x1b\\\x1b]110\x1b\\\x1b]111\x1b\\\x1b]117\x1b\\\x1b]119\x1b\\".to_vec()
    };
    let terminals = terminals(env, Path::new("/proc"));
    let mut sent = false;
    for terminal in terminals {
        if send(env, &terminal, &commands).is_err() {
            return "reload-needed".into();
        }
        sent = true;
    }
    if enabled && sent { "reload-sent" } else { "on-next-open" }.into()
}

#[cfg(test)]
#[path = "foot_tests.rs"]
mod tests;
