use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

const CONFIG_MODE: u32 = 0o600;
const PERMISSION_MASK: u32 = 0o777;
const RECEIPT_VERSION: u32 = 1;

#[derive(Deserialize, Serialize)]
pub(super) struct Patch {
    pub before: String,
    pub after: String,
    #[serde(default)]
    pub retained_section: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct Receipt {
    version: u32,
    config: PathBuf,
    pending: bool,
    created: bool,
    patches: Vec<Patch>,
}

pub(super) fn check(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        match std::fs::symlink_metadata(ancestor) {
            Ok(meta) => {
                ensure!(!meta.file_type().is_symlink(), "Symlink path: {}", ancestor.display());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

pub(super) fn read(path: &Path) -> Result<Option<String>> {
    check(path)?;
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("Read {}", path.display())),
    }
}

pub(super) fn write(path: &Path, text: &str) -> Result<()> {
    check(path)?;
    std::fs::create_dir_all(path.parent().context("Missing config parent")?)?;
    let mode = match std::fs::metadata(path) {
        Ok(meta) => {
            ensure!(!meta.permissions().readonly(), "Config is read-only: {}", path.display());
            meta.permissions().mode() & PERMISSION_MASK
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => CONFIG_MODE,
        Err(error) => return Err(error.into()),
    };
    crate::paths::atomic_write_mode(path, text.as_bytes(), Some(mode))?;
    Ok(())
}

pub(super) fn manage(
    path: &Path,
    record: &Path,
    enabled: bool,
    patches: impl FnOnce(&str) -> Result<Vec<Patch>>,
) -> Result<()> {
    let saved = read(record)?;
    if saved.is_none() && !enabled {
        return super::backup::end(record);
    }
    super::backup::prepare(path, record)?;
    let original = read(path)?;
    let current = original.as_deref().unwrap_or_default();
    if let Some(saved) = saved {
        let mut receipt: Receipt = serde_json::from_str(&saved)?;
        ensure!(receipt.version == RECEIPT_VERSION, "Unsupported lockscreen receipt");
        ensure!(receipt.config == path, "Lockscreen config path changed");
        let restored = receipt
            .patches
            .iter()
            .rev()
            .try_fold(current.to_owned(), |text, patch| restore(&text, patch));
        let restored = match restored {
            Ok(restored) => restored,
            Err(_) if receipt.pending => {
                let installed = apply(current, &receipt.patches)?;
                if enabled {
                    finish(path, record, &mut receipt, &installed)?;
                } else {
                    std::fs::remove_file(record)?;
                    super::backup::end(record)?;
                }
                return Ok(());
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("Lockscreen setting changed outside Skwd: {}", path.display())
                });
            }
        };
        if enabled {
            if receipt.pending {
                receipt.pending = false;
                write(record, &serde_json::to_string(&receipt)?)?;
            }
            return Ok(());
        }
        if receipt.created && restored.is_empty() {
            std::fs::remove_file(path)?;
        } else {
            write(path, &restored)?;
        }
        std::fs::remove_file(record)?;
        super::backup::end(record)?;
        return Ok(());
    }
    if !enabled {
        return Ok(());
    }
    let patches = patches(current)?;
    let installed = apply(current, &patches)?;
    let mut receipt = Receipt {
        version: RECEIPT_VERSION,
        config: path.to_path_buf(),
        pending: true,
        created: original.is_none(),
        patches,
    };
    write(record, &serde_json::to_string(&receipt)?)?;
    finish(path, record, &mut receipt, &installed)
}

fn finish(path: &Path, record: &Path, receipt: &mut Receipt, installed: &str) -> Result<()> {
    write(path, installed)?;
    receipt.pending = false;
    write(record, &serde_json::to_string(receipt)?)
}

fn apply(current: &str, patches: &[Patch]) -> Result<String> {
    let mut installed = current.to_owned();
    for patch in patches {
        if patch.before.is_empty() {
            ensure!(!installed.contains(&patch.after), "Ambiguous lockscreen setting");
            installed.insert_str(0, &patch.after);
        } else {
            ensure!(installed.matches(&patch.before).count() == 1, "Ambiguous lockscreen setting");
            installed = installed.replacen(&patch.before, &patch.after, 1);
        }
    }
    Ok(installed)
}

fn restore(current: &str, patch: &Patch) -> Result<String> {
    if let Some(header) = &patch.retained_section {
        return restore_section(current, patch, header);
    }
    if current.matches(&patch.after).count() == 1 {
        return Ok(current.replacen(&patch.after, &patch.before, 1));
    }
    let before: Vec<_> = patch.before.split_inclusive('\n').collect();
    let after: Vec<_> = patch.after.split_inclusive('\n').collect();
    ensure!(before.len() == after.len(), "Owned directive changed");
    let changed: Vec<_> = before.iter().zip(&after).filter(|(old, new)| old != new).collect();
    let [(old, new)] = changed.as_slice() else { anyhow::bail!("Owned directive changed") };
    ensure!(current.matches(**new).count() == 1, "Owned directive changed");
    Ok(current.replacen(**new, old, 1))
}

fn restore_section(current: &str, patch: &Patch, header: &str) -> Result<String> {
    ensure!(
        (patch.before.is_empty() || patch.before.trim_end() == header.trim_end())
            && patch.after.starts_with(header),
        "Invalid owned INI section"
    );
    ensure!(current.matches(header).count() == 1, "Owned INI section changed");
    let start = current.find(header).context("Missing owned INI section")?;
    let body_start = start + header.len();
    let mut end = body_start;
    for line in current[body_start..].split_inclusive('\n') {
        if line.trim_start().starts_with('[') {
            break;
        }
        end += line.len();
    }
    let mut body = current[body_start..end].to_owned();
    for line in patch.after[header.len()..].split_inclusive('\n') {
        ensure!(
            body.split_inclusive('\n').filter(|entry| *entry == line).count() == 1,
            "Owned INI directive changed"
        );
        body = body.split_inclusive('\n').filter(|entry| *entry != line).collect();
    }
    let section = if body.is_empty() {
        patch.before.clone()
    } else if body.trim().is_empty() && patch.before.is_empty() {
        body
    } else {
        format!("{header}{body}")
    };
    Ok(format!("{}{section}{}", &current[..start], &current[end..]))
}
