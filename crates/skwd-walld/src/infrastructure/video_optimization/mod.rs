mod ffmpeg;

pub(crate) use ffmpeg::{
    decode_probe_ok, probe, probe_duration_ms, probe_frame_rate, spawn_encoder, tinier_dest_path,
    tinier_encode_args,
};

pub(crate) fn retire_general_cache(state: &skwd_wall_core::WallState) {
    let records =
        state.with_db(skwd_wall_core::db::retire_video_converts).unwrap_or_default().len();
    let root = std::path::Path::new(&state.config().cache_dir()).join("video-opt");
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
            "retired general video optimization: cleared {records} records and {removed} cached files ({:.1} MiB); Tinier artifacts retained",
            bytes as f64 / 1_048_576.0
        );
    }
}

fn is_general_variant(root: &std::path::Path, path: &std::path::Path) -> bool {
    path.parent() == Some(root)
        && path.file_name().and_then(std::ffi::OsStr::to_str).is_some_and(|name| {
            [".av1.mp4", ".vp9.mp4", ".h264-lean.mp4"].iter().any(|suffix| name.ends_with(suffix))
        })
}

#[cfg(test)]
use ffmpeg::{OPT_THREADS, decode_probe_args, frame_rate_from_probe};

#[cfg(test)]
mod tests;
