use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use super::catalogue::Recipe;

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct Receipt {
    pub version: u32,
    pub enabled: bool,
    pub pending: bool,
    #[serde(default)]
    pub pending_config: Option<String>,
    pub config: PathBuf,
    pub output: PathBuf,
    pub original: Option<String>,
    pub before: String,
    pub after: String,
    pub rendered: String,
    pub result: String,
}

pub(super) fn read(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("Cannot read {}", path.display())),
    }
}

pub(super) fn writable(path: &Path) -> Result<()> {
    let mut checked_permissions = false;
    for ancestor in path.ancestors() {
        match std::fs::symlink_metadata(ancestor) {
            Ok(meta) => {
                ensure!(
                    !meta.file_type().is_symlink(),
                    "Config is managed through a symlink: {}",
                    ancestor.display()
                );
                ensure!(
                    checked_permissions || !meta.permissions().readonly(),
                    "Config is read-only: {}",
                    ancestor.display()
                );
                checked_permissions = true;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

pub(super) fn write(path: &Path, text: &str) -> Result<()> {
    writable(path)?;
    let parent = path.parent().context("Missing config directory")?;
    std::fs::create_dir_all(parent)?;
    let mode = std::fs::metadata(path).map_or(0o600, |meta| meta.permissions().mode() & 0o777);
    crate::paths::atomic_write_mode(path, text.as_bytes(), Some(mode))?;
    Ok(())
}

pub(super) fn load(path: &Path) -> Result<Option<Receipt>> {
    let Some(text) = read(path)? else { return Ok(None) };
    let receipt: Receipt = serde_json::from_str(&text).context("Cannot read saved theme setup")?;
    ensure!(receipt.version == 1, "Unsupported saved theme setup version");
    Ok(Some(receipt))
}

pub(super) fn save(path: &Path, receipt: &Receipt) -> Result<()> {
    write(path, &serde_json::to_string_pretty(receipt)?)
}

fn theme_line(line: &str) -> bool {
    line.trim_start().split_once('=').is_some_and(|(key, _)| key.trim() == "color_theme")
}

pub(super) fn patch(recipe: &Recipe, original: &str) -> Result<(String, String)> {
    if recipe.id == "btop" {
        let lines: Vec<_> =
            original.split_inclusive('\n').filter(|line| theme_line(line)).collect();
        ensure!(lines.len() <= 1, "Multiple btop theme selections need review");
        if let Some(before) = lines.first() {
            let after =
                format!("{}{}", recipe.directive, if before.ends_with('\n') { "\n" } else { "" });
            return Ok(((*before).to_string(), after));
        }
    }
    let (open, close) = match recipe.id {
        "niri" | "rofi" => ("// ", ""),
        "waybar" => ("/* ", " */"),
        _ => ("# ", ""),
    };
    ensure!(!original.contains("Skwd app theme"), "An existing Skwd setup needs review");
    let separator = if original.is_empty() || original.ends_with('\n') { "" } else { "\n" };
    Ok((
        String::new(),
        format!(
            "{separator}{open}Skwd app theme{close}\n{}\n{open}End Skwd app theme{close}\n",
            recipe.directive
        ),
    ))
}

pub(super) fn changed(original: &str, before: &str, after: &str) -> Result<String> {
    if before.is_empty() {
        return Ok(format!("{original}{after}"));
    }
    ensure!(
        original.matches(before).count() == 1,
        "The theme setting changed outside Skwd; review the config"
    );
    Ok(original.replacen(before, after, 1))
}

pub(super) fn restore(receipt: &Receipt, recipe: &Recipe, current: &str) -> Result<String> {
    if current.matches(&receipt.after).count() == 1 {
        return Ok(current.replacen(&receipt.after, &receipt.before, 1));
    }
    if recipe.id == "btop" {
        let lines: Vec<_> = current.split_inclusive('\n').filter(|line| theme_line(line)).collect();
        if lines.len() == 1 && lines[0].trim() == recipe.directive {
            return Ok(current.replacen(lines[0], &receipt.before, 1));
        }
    }
    bail!("The theme setting changed outside Skwd; review the config before restoring")
}
