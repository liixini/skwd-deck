use std::ops::ControlFlow;
use std::path::Path;

use anyhow::{Context, anyhow};
use ffmpeg_the_third as ff;
use image::DynamicImage;

use super::super::images::{preshrink_dims, write_thumbs};
use super::super::{THUMB_H, THUMB_W, ThumbResult};
use super::scaling::CoverScaler;
use super::source::VideoSource;

pub fn generate_video_thumbs(
    source: &Path,
    thumbnail: &Path,
    small_thumbnail: &Path,
    seek_seconds: i64,
) -> anyhow::Result<ThumbResult> {
    let (image, source_width, source_height, duration_ms) =
        decode_video_thumb_frame(source, seek_seconds)?;
    let mut result = write_thumbs(image, thumbnail, small_thumbnail)?;
    result.width = source_width;
    result.height = source_height;
    result.duration_ms = duration_ms;
    Ok(result)
}

pub fn extract_frame_to(
    source: &Path,
    destination: &Path,
    seek_seconds: i64,
) -> anyhow::Result<()> {
    let image = decode_video_frame(source, seek_seconds)?;
    if let Some(directory) = destination.parent() {
        std::fs::create_dir_all(directory).ok();
    }
    image.save(destination).with_context(|| format!("write frame {}", destination.display()))?;
    Ok(())
}

pub(crate) fn mean_plane_value(data: &[u8], stride: usize, width: usize, height: usize) -> f32 {
    if data.is_empty() || stride == 0 || width == 0 || height == 0 {
        return 0.0;
    }
    let step = (height / 32).max(1);
    let mut sum = 0u64;
    let mut count = 0u64;
    let mut row = 0;
    while row < height {
        let start = row * stride;
        if start >= data.len() {
            break;
        }
        let end = (start + width).min(data.len());
        let pixels = &data[start..end];
        sum += pixels.iter().map(|&byte| u64::from(byte)).sum::<u64>();
        count += pixels.len() as u64;
        row += step;
    }
    if count == 0 { 0.0 } else { sum as f32 / count as f32 }
}

fn mean_luma(frame: &ff::frame::Video) -> f32 {
    mean_plane_value(
        frame.data(0),
        frame.stride(0),
        frame.width() as usize,
        frame.height() as usize,
    )
}

fn grab_frame_near(
    source: &mut VideoSource,
    decoder: &mut ff::decoder::Video,
    target_seconds: f64,
) -> (Option<ff::frame::Video>, Option<ff::frame::Video>) {
    let target_us = (target_seconds * 1_000_000.0) as i64;
    decoder.flush();
    let _ = source.input.seek(target_us, ..target_us);
    let mut best = None;
    let mut best_luma = -1.0f32;
    let mut skipped = 0usize;
    let mut evaluated = 0usize;
    for result in source.input.packets() {
        let Ok((stream, packet)) = result else { break };
        if stream.index() != source.stream_index || decoder.send_packet(&packet).is_err() {
            continue;
        }
        let scanned = scan_frames(
            decoder,
            source.time_base_secs,
            target_seconds,
            &mut best,
            &mut best_luma,
            &mut skipped,
            &mut evaluated,
        );
        if let ControlFlow::Break(found) = scanned {
            return (found, best);
        }
    }
    (None, best)
}

fn scan_frames(
    decoder: &mut ff::decoder::Video,
    time_base_seconds: f64,
    target_seconds: f64,
    best: &mut Option<ff::frame::Video>,
    best_luma: &mut f32,
    skipped: &mut usize,
    evaluated: &mut usize,
) -> ControlFlow<Option<ff::frame::Video>> {
    const MIN_LUMA: f32 = 24.0;
    const MAX_EVALUATED: usize = 24;
    const MAX_SKIPPED: usize = 240;
    loop {
        let mut frame = ff::frame::Video::empty();
        if decoder.receive_frame(&mut frame).is_err() {
            return ControlFlow::Continue(());
        }
        let frame_seconds = frame.pts().unwrap_or(0) as f64 * time_base_seconds;
        if frame_seconds + 0.01 < target_seconds && *skipped < MAX_SKIPPED {
            *skipped += 1;
            continue;
        }
        let luma = mean_luma(&frame);
        if luma >= MIN_LUMA {
            return ControlFlow::Break(Some(frame));
        }
        if luma > *best_luma {
            *best_luma = luma;
            *best = Some(frame);
        }
        *evaluated += 1;
        if *evaluated >= MAX_EVALUATED {
            return ControlFlow::Break(None);
        }
    }
}

pub(crate) fn decode_video_frame(
    source_path: &Path,
    seek_seconds: i64,
) -> anyhow::Result<DynamicImage> {
    let decoded = decode_selected_video_frame(source_path, seek_seconds)?;
    Ok(DynamicImage::ImageRgb8(frame_to_rgb(&decoded)?))
}

fn decode_video_thumb_frame(
    source_path: &Path,
    seek_seconds: i64,
) -> anyhow::Result<(DynamicImage, u32, u32, i64)> {
    let (decoded, duration_ms) =
        decode_selected_video_frame_with_duration(source_path, seek_seconds)?;
    let (source_width, source_height) = (decoded.width(), decoded.height());
    let (target_width, target_height) =
        if preshrink_dims(source_width, source_height, THUMB_W, THUMB_H).is_some() {
            (THUMB_W * 2, THUMB_H * 2)
        } else {
            (THUMB_W, THUMB_H)
        };
    let mut scaler = CoverScaler::new_with_flags(
        target_width,
        target_height,
        ff::format::Pixel::RGB24,
        3,
        ff::software::scaling::Flags::LANCZOS,
    );
    let image = scaler.cover_rgb(&decoded)?;
    Ok((DynamicImage::ImageRgb8(image), source_width, source_height, duration_ms))
}

fn decode_selected_video_frame(
    source: &Path,
    seek_seconds: i64,
) -> anyhow::Result<ff::frame::Video> {
    decode_selected_video_frame_with_duration(source, seek_seconds).map(|(frame, _)| frame)
}

fn decode_selected_video_frame_with_duration(
    source_path: &Path,
    seek_seconds: i64,
) -> anyhow::Result<(ff::frame::Video, i64)> {
    let mut source = VideoSource::open(source_path)?;
    let mut decoder = source.decoder()?;
    let duration_us = source.duration_us();
    let duration_seconds = duration_us as f64 / 1_000_000.0;
    let mut targets = vec![seek_seconds.max(0) as f64];
    if duration_seconds > 4.0 {
        targets.push(duration_seconds * 0.25);
        targets.push(duration_seconds * 0.5);
    }
    let decoded = pick_frame(&mut source, &mut decoder, &targets)
        .map_or_else(|| flush_frame(&mut decoder, source_path), Ok)?;
    Ok((decoded, duration_us / 1_000))
}

fn pick_frame(
    source: &mut VideoSource,
    decoder: &mut ff::decoder::Video,
    targets: &[f64],
) -> Option<ff::frame::Video> {
    let mut overall_best = None;
    let mut overall_best_luma = -1.0f32;
    for &target in targets {
        let (bright, dim) = grab_frame_near(source, decoder, target);
        if let Some(frame) = bright {
            return Some(frame);
        }
        if let Some(frame) = dim {
            let luma = mean_luma(&frame);
            if luma > overall_best_luma {
                overall_best_luma = luma;
                overall_best = Some(frame);
            }
        }
    }
    overall_best
}

fn flush_frame(
    decoder: &mut ff::decoder::Video,
    source: &Path,
) -> anyhow::Result<ff::frame::Video> {
    let _ = decoder.send_eof();
    let mut frame = ff::frame::Video::empty();
    if decoder.receive_frame(&mut frame).is_err() {
        anyhow::bail!("no decodable frame in {}", source.display());
    }
    Ok(frame)
}

fn frame_to_rgb(frame: &ff::frame::Video) -> anyhow::Result<image::RgbImage> {
    let mut scaler = ff::software::scaling::Context::get(
        frame.format(),
        frame.width(),
        frame.height(),
        ff::format::Pixel::RGB24,
        frame.width(),
        frame.height(),
        ff::software::scaling::Flags::BILINEAR,
    )?;
    let mut rgb = ff::frame::Video::empty();
    scaler.run(frame, &mut rgb)?;
    let (width, height, stride) = (rgb.width(), rgb.height(), rgb.stride(0));
    let row_bytes = width as usize * 3;
    let mut buffer = Vec::with_capacity(row_bytes * height as usize);
    for row in 0..height as usize {
        let start = row * stride;
        buffer.extend_from_slice(&rgb.data(0)[start..start + row_bytes]);
    }
    image::RgbImage::from_raw(width, height, buffer)
        .ok_or_else(|| anyhow!("frame buffer size mismatch"))
}
