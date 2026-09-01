use std::path::Path;

use anyhow::{Context, anyhow};
use ffmpeg_the_third as ff;

use super::super::{THUMB_H, THUMB_W};
use super::preview_policy::{
    PREVIEW_FPS_CAP, PREVIEW_MAX_FRAMES, PREVIEW_QUALITY, PREVIEW_SECONDS, frame_duration_ms,
    keep_preview_frame,
};
use super::scaling::CoverScaler;
use super::source::VideoSource;

pub fn generate_video_preview(source_path: &Path, destination: &Path) -> anyhow::Result<usize> {
    let frames = decode_preview_frames(source_path)?;
    if frames.len() < 2 {
        anyhow::bail!("insufficient preview frames in {}", source_path.display());
    }
    let timestamps: Vec<f64> = frames.iter().map(|(_, timestamp)| *timestamp).collect();
    let mut config = webp::WebPConfig::new().map_err(|()| anyhow!("webp config init failed"))?;
    config.quality = PREVIEW_QUALITY;
    let mut encoder = webp::AnimEncoder::new(THUMB_W, THUMB_H, &config);
    encoder.set_loop_count(0);
    let mut elapsed_ms = 0i32;
    for (index, (image, _)) in frames.iter().enumerate() {
        encoder.add_frame(webp::AnimFrame::from_rgb(image.as_raw(), THUMB_W, THUMB_H, elapsed_ms));
        elapsed_ms += frame_duration_ms(&timestamps, index);
    }
    let data = encoder.try_encode().map_err(|error| anyhow!("anim encode failed: {error:?}"))?;
    if let Some(directory) = destination.parent() {
        std::fs::create_dir_all(directory).ok();
    }
    crate::paths::atomic_write(destination, &data)
        .with_context(|| format!("write {}", destination.display()))?;
    Ok(frames.len())
}

fn decode_preview_frames(source_path: &Path) -> anyhow::Result<Vec<(image::RgbImage, f64)>> {
    let mut source = VideoSource::open(source_path)?;
    let mut decoder = source.decoder()?;
    let start_us = 1_000_000i64;
    let _ = source.input.seek(start_us, ..start_us);
    let mut frames = Vec::with_capacity(PREVIEW_MAX_FRAMES);
    let mut scaler = CoverScaler::new(THUMB_W, THUMB_H, ff::format::Pixel::RGB24, 3);
    let mut start_pts = f64::NEG_INFINITY;
    let mut last_kept = f64::NEG_INFINITY;
    for result in source.input.packets() {
        let Ok((stream, packet)) = result else { break };
        if stream.index() != source.stream_index || decoder.send_packet(&packet).is_err() {
            continue;
        }
        if take_preview_frames(
            &mut decoder,
            source.time_base_secs,
            &mut scaler,
            &mut frames,
            &mut start_pts,
            &mut last_kept,
        ) {
            break;
        }
    }
    Ok(frames)
}

fn take_preview_frames(
    decoder: &mut ff::decoder::Video,
    time_base_seconds: f64,
    scaler: &mut CoverScaler,
    frames: &mut Vec<(image::RgbImage, f64)>,
    start_pts: &mut f64,
    last_kept: &mut f64,
) -> bool {
    loop {
        let mut frame = ff::frame::Video::empty();
        if decoder.receive_frame(&mut frame).is_err() {
            return false;
        }
        let frame_seconds = frame.pts().unwrap_or(0) as f64 * time_base_seconds;
        if !start_pts.is_finite() {
            *start_pts = frame_seconds;
        }
        if !keep_preview_frame(*last_kept, frame_seconds, PREVIEW_FPS_CAP) {
            continue;
        }
        if let Ok(image) = scaler.cover_rgb(&frame) {
            *last_kept = frame_seconds;
            frames.push((image, frame_seconds));
            if frames.len() >= PREVIEW_MAX_FRAMES || frame_seconds - *start_pts >= PREVIEW_SECONDS {
                return true;
            }
        }
    }
}
