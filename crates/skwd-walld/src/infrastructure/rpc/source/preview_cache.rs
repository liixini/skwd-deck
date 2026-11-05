use std::path::Path;
use std::sync::{Mutex, PoisonError};

const MAX_BYTES: u64 = 256 * 1024 * 1024;
const MAX_ENTRIES: usize = 128;
const STEAM_HASH_LEN: usize = 16;
const MAX_EXTENSION_LEN: usize = 5;
static PRUNE_LOCK: Mutex<()> = Mutex::new(());

fn managed_file(path: &Path, steam: bool) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some((stem, extension)) = name.rsplit_once('.') else {
        return false;
    };
    if extension.is_empty()
        || extension.len() > MAX_EXTENSION_LEN
        || !extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        return false;
    }
    let owned = if steam {
        if !stem.is_empty() && stem.bytes().all(|byte| byte.is_ascii_digit()) {
            true
        } else {
            let Some((id, hash)) = stem.rsplit_once('-') else {
                return false;
            };
            !id.is_empty()
                && id.bytes().all(|byte| byte.is_ascii_digit())
                && hash.len() == STEAM_HASH_LEN
                && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        }
    } else {
        !stem.is_empty()
            && stem.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    };
    owned
        && crate::infrastructure::sniff::check_file(path, crate::infrastructure::sniff::Kind::Image)
            .is_ok()
}

fn prune_old(current: &Path, max_bytes: u64, max_entries: usize) -> anyhow::Result<()> {
    let _guard = PRUNE_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let Some(directory) = current.parent() else {
        return Ok(());
    };
    let steam = directory
        .file_name()
        .is_some_and(|name| name == wall_proto::sources::Provider::Steam.key());
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if !managed_file(&path, steam) || !entry.file_type()?.is_file() {
            continue;
        }
        let metadata = entry.metadata()?;
        entries.push((path, metadata.len(), metadata.modified()?));
    }
    let mut bytes = entries.iter().map(|(_, size, _)| size).sum::<u64>();
    let mut count = entries.len();
    entries.sort_by_key(|(_, _, modified)| *modified);
    for (path, size, _) in entries {
        if bytes <= max_bytes && count <= max_entries {
            break;
        }
        if path == current {
            continue;
        }
        std::fs::remove_file(path)?;
        bytes -= size;
        count -= 1;
    }
    Ok(())
}

pub(super) fn prune(current: &Path) -> anyhow::Result<()> {
    prune_old(current, MAX_BYTES, MAX_ENTRIES)
}

#[cfg(test)]
#[path = "preview_cache/tests.rs"]
mod tests;
