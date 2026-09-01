use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn scan_error_marker(thumbnail: &Path) -> PathBuf {
    PathBuf::from(format!("{}.err", thumbnail.to_string_lossy()))
}

pub(super) fn write_scan_error(thumbnail: &Path, modified_at: i64) {
    let marker = scan_error_marker(thumbnail);
    if let Some(parent) = marker.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&marker, modified_at.to_string());
}

pub(super) fn clear_scan_error(thumbnail: &Path) {
    let _ = std::fs::remove_file(scan_error_marker(thumbnail));
}

pub(super) fn scan_error_fresh(thumbnail: &Path, modified_at: i64) -> bool {
    std::fs::read_to_string(scan_error_marker(thumbnail))
        .ok()
        .and_then(|text| text.trim().parse::<i64>().ok())
        .is_some_and(|recorded| recorded == modified_at)
}

pub(super) fn image_needs_thumb(
    known: &HashMap<String, i64>,
    key: &str,
    modified_at: i64,
    artifacts_ready: bool,
    error_fresh: bool,
) -> bool {
    let unchanged = known.get(key).copied() == Some(modified_at);
    if unchanged && artifacts_ready {
        return false;
    }
    !error_fresh
}
