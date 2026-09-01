use std::io::Write;
use std::path::Path;

use serde_json::json;
use skwd_wall_core::{media, paths};
use wall_proto::rpc;

use crate::reporter::Reporter;

pub(crate) fn stream_persist() {
    set_preview_process_name();
    let stdout = std::io::stdout();
    let mut writer = std::io::BufWriter::new(stdout.lock());
    if let Err(error) = media::stream_video_frames_persist(640, 360, &mut writer) {
        log::warn!("stream-persist ended: {error}");
    }
    let _ = writer.flush();
}

#[cfg(target_os = "linux")]
fn set_preview_process_name() {
    const NAME: &[u8] = b"skwd-wall-prev\0";
    unsafe {
        libc::prctl(libc::PR_SET_NAME, NAME.as_ptr().cast::<libc::c_char>());
    }
}

#[cfg(not(target_os = "linux"))]
fn set_preview_process_name() {}

pub(crate) fn stream_video(video: &str) {
    let stdout = std::io::stdout();
    let mut writer = std::io::BufWriter::new(stdout.lock());
    if let Err(error) = media::stream_video_frames(Path::new(video), 640, 360, &mut writer) {
        log::warn!("stream failed for {video}: {error}");
    }
    let _ = writer.flush();
}

pub(crate) fn generate_preview(key: &str, video: &str, reporter: &Reporter) {
    let Some(destination) = paths::preview_for_key(key) else {
        log::warn!("preview: no preview path for key {key}");
        return;
    };
    if destination.exists() {
        reporter.send(
            rpc::PREVIEW_READY,
            &json!({ "key": key, "path": destination.to_string_lossy() }),
        );
        return;
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    match media::generate_video_preview(Path::new(video), &destination) {
        Ok(count) => {
            log::info!("preview: {count} frames for {key}");
            reporter.send(
                rpc::PREVIEW_READY,
                &json!({ "key": key, "path": destination.to_string_lossy() }),
            );
        }
        Err(error) => log::warn!("preview failed for {key}: {error}"),
    }
}
