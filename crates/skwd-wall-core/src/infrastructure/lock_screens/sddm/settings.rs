use std::fmt::Write;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, ensure};

use super::{Paths, storage};
use crate::infrastructure::lock_screens::ownership::{self, Patch};

const CONFIG_DIRS: [&str; 2] = ["usr/lib/sddm/sddm.conf.d", "etc/sddm.conf.d"];
pub(super) const CONFIG: &str = "etc/sddm.conf";
const EXTENSION: &str = "conf";
const THEME: &str = "Theme";
const CURRENT: &str = "Current";
const THEME_DIR: &str = "ThemeDir";
const DEFAULT_THEMES: &str = "/usr/share/sddm/themes";
const SUPPORTED: [&str; 3] = ["breeze", "elarun", "maldives"];
const THEME_CONFIG: &str = "theme.conf";
const USER_CONFIG: &str = "theme.conf.user";
const METADATA: &str = "metadata.desktop";
const METADATA_GROUP: &str = "SddmGreeterTheme";
const CONFIG_FILE: &str = "ConfigFile";
const GENERAL: &str = "General";
const BACKGROUND: &str = "background";
const TYPE: &str = "type";
const IMAGE: &str = "image";
const BOM: char = '\u{feff}';

fn section(line: &str) -> Result<Option<&str>> {
    let line = line.trim().trim_start_matches(BOM);
    let Some(line) = line.strip_prefix('[') else { return Ok(None) };
    let (name, suffix) = line.split_once(']').context("Invalid SDDM section")?;
    let suffix = suffix.trim();
    ensure!(
        suffix.is_empty() || suffix.starts_with(';') || suffix.starts_with('#'),
        "Unsupported SDDM section formatting"
    );
    let name = name.trim();
    ensure!(!name.contains(['%', '\\']), "Unsupported escaped SDDM section");
    Ok(Some(name))
}

fn matches_section(name: &str, group: &str) -> bool {
    name == group || (group == GENERAL && name.eq_ignore_ascii_case(GENERAL))
}

fn field(line: &str) -> Result<Option<(&str, &str)>> {
    if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
        return Ok(None);
    }
    let (name, value) = line.split_once('=').context("Unsupported SDDM entry")?;
    ensure!(!name.contains(['%', '\\']), "Unsupported escaped SDDM setting");
    let mut quoted = false;
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            quoted = !quoted;
        }
    }
    ensure!(!quoted && !escaped, "Multiline SDDM values are unsupported");
    Ok(Some((name, value)))
}

fn entry(text: &str, group: &str, key: &str) -> Result<Option<String>> {
    let mut selected = false;
    let mut found = false;
    let mut value = None;
    for line in text.lines().map(str::trim) {
        if let Some(name) = section(line)? {
            selected = matches_section(name, group);
            ensure!(!selected || !found, "Duplicate SDDM section {group}");
            found |= selected;
        } else if let Some((name, next)) = field(line)? {
            if !selected {
                continue;
            }
            if name.trim() == key {
                ensure!(value.is_none(), "Duplicate SDDM setting {key}");
                value = Some(next.trim().into());
            }
        }
    }
    Ok(value)
}

pub(super) fn selection(paths: &Paths) -> Result<(String, PathBuf)> {
    let mut files = Vec::new();
    for dir in CONFIG_DIRS {
        let dir = paths.root.join(dir);
        storage::trusted(paths, &dir)?;
        if dir.exists() {
            let mut entries = std::fs::read_dir(dir)?
                .map(|entry| entry.map(|entry| entry.path()))
                .collect::<std::io::Result<Vec<_>>>()?;
            entries.retain(|path| path.extension().is_some_and(|extension| extension == EXTENSION));
            entries.sort();
            files.extend(entries);
        }
    }
    files.push(paths.root.join(CONFIG));
    let mut current = String::new();
    let mut directory = DEFAULT_THEMES.to_owned();
    for file in files {
        storage::trusted(paths, &file)?;
        if let Some(text) = ownership::read(&file)? {
            if let Some(next) = entry(&text, THEME, CURRENT)? {
                current = next;
            }
            if let Some(next) = entry(&text, THEME, THEME_DIR)? {
                directory = next;
            }
        }
    }
    let directory = Path::new(&directory);
    ensure!(
        directory.is_absolute()
            && directory
                .components()
                .all(|part| matches!(part, Component::RootDir | Component::Normal(_))),
        "Invalid SDDM theme directory"
    );
    Ok((current, paths.root.join(directory.strip_prefix(super::SYSTEM_ROOT)?)))
}

pub(super) fn theme(paths: &Paths, name: &str) -> Result<PathBuf> {
    ensure!(!name.is_empty(), "The embedded SDDM theme needs an explicit installed theme choice");
    ensure!(SUPPORTED.contains(&name), "Unsupported SDDM theme: {name}");
    let (_, directory) = selection(paths)?;
    let theme = directory.join(name);
    let metadata = theme.join(METADATA);
    storage::trusted(paths, &metadata)?;
    let metadata = ownership::read(&metadata)?.context("SDDM theme metadata is missing")?;
    ensure!(
        entry(&metadata, METADATA_GROUP, CONFIG_FILE)?.as_deref() == Some(THEME_CONFIG),
        "Unsupported SDDM theme configuration file"
    );
    let defaults = theme.join(THEME_CONFIG);
    storage::trusted(paths, &defaults)?;
    let text = ownership::read(&defaults)?.context("SDDM theme configuration is missing")?;
    ensure!(entry(&text, GENERAL, BACKGROUND)?.is_some(), "SDDM theme has no background setting");
    let config = theme.join(USER_CONFIG);
    storage::trusted(paths, &config)?;
    Ok(config)
}

pub(super) fn patches(text: &str, group: &str, fields: &[(&str, &str)]) -> Result<Vec<Patch>> {
    let mut patches = Vec::new();
    let mut active = false;
    let mut found = false;
    let mut original_header = None;
    let mut present = Vec::new();
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if let Some(name) = section(trimmed)? {
            active = matches_section(name, group);
            ensure!(!active || !found, "Duplicate SDDM section {group}");
            found |= active;
            if active {
                original_header = Some(line);
            }
        } else if let Some((name, _)) = field(trimmed)? {
            if !active {
                continue;
            }
            for &(key, value) in fields {
                if name.trim() == key {
                    ensure!(!present.contains(&key), "Duplicate SDDM setting {key}");
                    present.push(key);
                    patches.push(Patch {
                        before: line.to_owned(),
                        after: format!("{key}={value}\n"),
                        retained_section: None,
                    });
                }
            }
        }
    }
    let mut inserted = String::new();
    for &(key, value) in fields {
        if !present.contains(&key) {
            writeln!(inserted, "{key}={value}")?;
        }
    }
    if !inserted.is_empty() {
        let header = format!("[{group}]\n");
        if let Some(original) = original_header {
            let header = format!("{}\n", original.trim_end_matches('\n'));
            patches.push(Patch {
                before: original.to_owned(),
                after: format!("{header}{inserted}"),
                retained_section: Some(header),
            });
        } else {
            patches.push(Patch {
                before: String::new(),
                after: format!("{header}{inserted}"),
                retained_section: Some(header),
            });
        }
    }
    Ok(patches)
}

pub(super) fn background(text: &str, image: &Path) -> Result<Vec<Patch>> {
    let image = image.to_str().context("Invalid SDDM image path")?;
    patches(text, GENERAL, &[(BACKGROUND, image), (TYPE, IMAGE)])
}

pub(super) fn current(text: &str, name: &str) -> Result<Vec<Patch>> {
    patches(text, THEME, &[(CURRENT, name)])
}

pub(super) fn choices(paths: &Paths) -> Result<super::Selection> {
    let (current, _) = selection(paths)?;
    let themes = SUPPORTED
        .iter()
        .filter(|name| theme(paths, name).is_ok())
        .map(|name| (*name).to_owned())
        .collect();
    Ok(super::Selection { current, themes })
}
