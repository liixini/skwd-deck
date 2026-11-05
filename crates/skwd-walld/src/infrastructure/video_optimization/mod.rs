use std::collections::{BTreeSet, HashMap};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use serde_json::Value;

const CACHE_FOLDER: &str = "video-opt";
const TINIER_SUFFIX: &str = ".tinier-v1.ivf";
const BYTES_PER_MIB: f64 = 1_048_576.0;

pub(crate) fn retire_general_cache(state: &skwd_wall_core::WallState) {
    let records =
        state.with_db(skwd_wall_core::db::retire_video_converts).unwrap_or_default().len();
    let root = Path::new(&state.config().cache_dir()).join(CACHE_FOLDER);
    let mut removed = 0_usize;
    let mut bytes = 0_u64;
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !is_general_variant(&root, &path) {
                continue;
            }
            let size = entry.metadata().map_or(0, |metadata| metadata.len());
            if std::fs::remove_file(&path).is_ok() {
                removed += 1;
                bytes = bytes.saturating_add(size);
            }
        }
    }
    if records > 0 || removed > 0 {
        log::info!(
            "retired general video optimization: cleared {records} records and {removed} cached files ({:.1} MiB)",
            bytes as f64 / BYTES_PER_MIB
        );
    }
}

fn is_general_variant(root: &Path, path: &Path) -> bool {
    path.parent() == Some(root)
        && path.file_name().and_then(OsStr::to_str).is_some_and(|name| {
            [".av1.mp4", ".vp9.mp4", ".h264-lean.mp4"].iter().any(|suffix| name.ends_with(suffix))
        })
}

pub(crate) fn retire_tinier(state: &skwd_wall_core::WallState) {
    let rows = match state.with_db(skwd_wall_core::db::retiring_tinier_converts) {
        Ok(Some(rows)) => rows,
        Ok(None) => return,
        Err(error) => {
            log::warn!("retired tinier: cannot read records: {error}");
            return;
        }
    };
    let originals: HashMap<&str, &str> = rows
        .iter()
        .filter(|(source, dest)| source != dest)
        .map(|(source, dest)| (dest.as_str(), source.as_str()))
        .collect();
    let mut repointed = 0_usize;
    for file in tinier_state_files(state) {
        match repoint_file(&file, &originals) {
            Ok(count) => repointed += count,
            Err(error) => {
                log::warn!("retired tinier: cannot update {}: {error}", file.display());
                return;
            }
        }
    }
    let mut removed = 0_usize;
    let mut bytes = 0_u64;
    let dests: Vec<&Path> =
        originals.keys().map(Path::new).filter(|path| in_cache_folder(path)).collect();
    for dest in &dests {
        let size = std::fs::metadata(dest).map_or(0, |metadata| metadata.len());
        if std::fs::remove_file(dest).is_ok() {
            removed += 1;
            bytes = bytes.saturating_add(size);
        }
    }
    let folders: BTreeSet<&Path> = dests.iter().filter_map(|dest| dest.parent()).collect();
    let (swept, swept_bytes) = folders
        .into_iter()
        .map(sweep_encodes)
        .fold((0, 0), |sum, next| (sum.0 + next.0, sum.1 + next.1));
    bytes = bytes.saturating_add(swept_bytes);
    if let Err(error) = state.with_db(skwd_wall_core::db::drop_tinier_converts) {
        log::warn!("retired tinier: cannot drop records: {error}");
    }
    if !rows.is_empty() {
        log::info!(
            "retired tinier: cleared {} records, repointed {repointed} saved paths, removed {removed} cached files and swept {swept} orphaned encodes ({:.1} MiB)",
            rows.len(),
            bytes as f64 / BYTES_PER_MIB
        );
    }
}

fn sweep_encodes(folder: &Path) -> (usize, u64) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return (0, 0);
    };
    let mut swept = 0_usize;
    let mut bytes = 0_u64;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| name.ends_with(TINIER_SUFFIX))
        {
            continue;
        }
        let size = entry.metadata().map_or(0, |metadata| metadata.len());
        if std::fs::remove_file(&path).is_ok() {
            swept += 1;
            bytes = bytes.saturating_add(size);
        }
    }
    (swept, bytes)
}

fn tinier_state_files(state: &skwd_wall_core::WallState) -> [PathBuf; 6] {
    let cache = state.config().cache_dir();
    [
        skwd_wall_core::audio::state_path(&cache),
        crate::infrastructure::history::history_path(&cache),
        crate::infrastructure::workspaces::base_file(&cache),
        crate::infrastructure::persistence::last_wallpaper_path(),
        crate::infrastructure::restore_policy::last_applied_path(),
        crate::infrastructure::restore_policy::monitors_path(),
    ]
}

fn repoint_file(file: &Path, originals: &HashMap<&str, &str>) -> std::io::Result<usize> {
    let Ok(text) = std::fs::read_to_string(file) else {
        return Ok(0);
    };
    let mut value = match serde_json::from_str::<Value>(&text) {
        Ok(value) => value,
        Err(error) => {
            log::warn!("retired tinier: cannot parse {}: {error}", file.display());
            return Ok(0);
        }
    };
    let count = repoint(&mut value, originals);
    if count > 0 {
        skwd_wall_core::paths::atomic_write(file, value.to_string().as_bytes())?;
    }
    Ok(count)
}

fn repoint(value: &mut Value, originals: &HashMap<&str, &str>) -> usize {
    match value {
        Value::String(text) => originals.get(text.as_str()).map_or(0, |original| {
            *text = (*original).to_string();
            1
        }),
        Value::Array(items) => items.iter_mut().map(|item| repoint(item, originals)).sum(),
        Value::Object(fields) => fields.values_mut().map(|item| repoint(item, originals)).sum(),
        _ => 0,
    }
}

fn in_cache_folder(path: &Path) -> bool {
    path.parent().and_then(Path::file_name) == Some(OsStr::new(CACHE_FOLDER))
}

#[cfg(test)]
mod tests;
