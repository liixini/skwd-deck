#![cfg(test)]

use std::path::Path;

use image::DynamicImage;

use super::images::{load_memory_with_limits, write_thumbs};
use super::video::{decode_video_frame, is_virtio_vendor, mean_plane_value};
use super::*;
use anyhow::anyhow;
use image::{Rgb, RgbImage};

#[test]
fn hue_bucket_values() {
    assert_eq!(hue_bucket(200, 9), 99);
    assert_eq!(hue_bucket(0, 50), 0);
    assert_eq!(hue_bucket(330, 50), 11);
    assert_eq!(hue_bucket(120, 50), 4);
}

#[test]
fn preshrink_above_2x() {
    assert_eq!(preshrink_dims(3840, 2160, THUMB_W, THUMB_H), Some((1280, 720)));
    assert_eq!(preshrink_dims(1280, 720, THUMB_W, THUMB_H), None);
    assert_eq!(preshrink_dims(800, 600, THUMB_W, THUMB_H), None);
    assert_eq!(preshrink_dims(0, 0, THUMB_W, THUMB_H), None);
    let (w, h) = preshrink_dims(8000, 2000, THUMB_W, THUMB_H).unwrap();
    assert!(h >= THUMB_H * 2);
    assert_eq!(w, 8000 * h / 2000);
}

#[test]
fn cover_dims_aspect() {
    assert_eq!(cover_dims(1920, 1080, 640, 360), (640, 360));
    let (cw, ch) = cover_dims(1080, 1920, 640, 360);
    assert!(cw >= 640 && ch >= 360);
    assert_eq!(cw, 640);
    assert!((f64::from(ch) / f64::from(cw) - 1920.0 / 1080.0).abs() < 0.01);
}

#[test]
fn preview_fps_cap() {
    let mut kept = 0;
    let mut last = f64::NEG_INFINITY;
    for idx in 0..180 {
        let t = idx as f64 / 60.0;
        if keep_preview_frame(last, t, PREVIEW_FPS_CAP) {
            kept += 1;
            last = t;
        }
    }
    assert!((55..=62).contains(&kept), "kept {kept}");
}

#[test]
fn thumbs_decodable() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src.png");
    let mut buf = RgbImage::new(800, 600);
    for (x, _y, p) in buf.enumerate_pixels_mut() {
        *p = if x < 400 { Rgb([200, 40, 40]) } else { Rgb([40, 40, 200]) };
    }
    buf.save(&src).unwrap();

    let thumb = dir.path().join("thumbs/src.webp");
    let small = dir.path().join("thumbs-sm/src.webp");
    let result = generate_image_thumbs(&src, &thumb, &small).unwrap();

    assert_eq!((result.width, result.height), (800, 600));
    assert!(thumb.exists() && small.exists());

    let decoded = image::open(&thumb).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (THUMB_W, THUMB_H));
    let decoded_sm = image::open(&small).unwrap();
    assert_eq!((decoded_sm.width(), decoded_sm.height()), (SMALL_W, SMALL_H));
}

#[test]
fn frame_duration_pts() {
    let pts = vec![1.0, 1.0 + 1.0 / 30.0, 1.0 + 2.0 / 30.0];
    assert_eq!(frame_duration_ms(&pts, 0), 33);
    assert_eq!(frame_duration_ms(&pts, 1), 33);
    assert_eq!(frame_duration_ms(&pts, 2), 33);
    assert_eq!(frame_duration_ms(&[5.0], 0), 42);
    assert_eq!(frame_duration_ms(&[0.0, 100.0], 0), 200);
    assert_eq!(frame_duration_ms(&[0.0, 0.0001], 0), 10);
}

#[test]
fn mean_plane_values() {
    assert_eq!(mean_plane_value(&[0; 8], 4, 4, 2), 0.0);
    assert_eq!(mean_plane_value(&[255; 8], 4, 4, 2), 255.0);
    let padded = [10u8, 10, 99, 99, 20, 20, 99, 99];
    assert_eq!(mean_plane_value(&padded, 4, 2, 2), 15.0);
    assert_eq!(mean_plane_value(&[], 0, 0, 0), 0.0);
}

#[test]
fn virtio_vendor_ids() {
    assert!(is_virtio_vendor("0x1af4\n"));
    assert!(is_virtio_vendor("0X1AF4"));
    assert!(!is_virtio_vendor("0x10de"));
}

#[test]
fn dav1d_decoder_is_linked() {
    ffmpeg_the_third::init().unwrap();
    assert!(ffmpeg_the_third::codec::decoder::find_by_name("libdav1d").is_some());
}

#[test]
fn image_dims_header() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("d.png");
    RgbImage::new(123, 45).save(&src).unwrap();
    assert_eq!(image_dimensions(&src), Some((123, 45)));
}

fn write_y4m(path: &Path, w: usize, h: usize, spans: &[(u8, usize)]) {
    let mut buf = format!("YUV4MPEG2 W{w} H{h} F25:1 Ip A1:1 C420jpeg\n").into_bytes();
    for &(luma, count) in spans {
        for _ in 0..count {
            buf.extend_from_slice(b"FRAME\n");
            buf.extend(std::iter::repeat_n(luma, w * h));
            buf.extend(std::iter::repeat_n(128u8, w * h / 2));
        }
    }
    std::fs::write(path, buf).unwrap();
}

fn mean_rgb(img: &DynamicImage) -> f64 {
    let rgb = img.to_rgb8();
    let sum: u64 = rgb.as_raw().iter().map(|&byte| u64::from(byte)).sum();
    sum as f64 / rgb.as_raw().len() as f64
}

#[test]
fn skips_dark_leader() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("clip.y4m");
    write_y4m(&src, 160, 120, &[(5, 10), (200, 40)]);
    let img = decode_video_frame(&src, 0).unwrap();
    assert_eq!((img.width(), img.height()), (160, 120));
    assert!(mean_rgb(&img) > 120.0);
}

#[test]
fn all_dark_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("dark.y4m");
    write_y4m(&src, 160, 120, &[(5, 20)]);
    let img = decode_video_frame(&src, 0).unwrap();
    assert_eq!((img.width(), img.height()), (160, 120));
    assert!(mean_rgb(&img) < 60.0);
}

#[test]
fn non_video_errors() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("noise.y4m");
    std::fs::write(&src, b"definitely not a video").unwrap();
    assert!(decode_video_frame(&src, 1).is_err());
    assert!(decode_video_frame(&dir.path().join("missing.y4m"), 1).is_err());
}

#[test]
fn video_thumbs_dims() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("clip.y4m");
    write_y4m(&src, 160, 120, &[(180, 40)]);
    let thumb = dir.path().join("video-thumbs/clip.webp");
    let small = dir.path().join("thumbs-sm/vid--clip.webp");
    let result = generate_video_thumbs(&src, &thumb, &small, 1).unwrap();
    assert_eq!((result.width, result.height), (160, 120));
    assert!(result.duration_ms >= 1_500);
    let decoded = image::open(&thumb).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (THUMB_W, THUMB_H));
    let decoded_sm = image::open(&small).unwrap();
    assert_eq!((decoded_sm.width(), decoded_sm.height()), (SMALL_W, SMALL_H));
}

#[test]
#[ignore = "manual scanner memory benchmark; set SKWD_BENCH_VIDEO"]
fn scanner_video_benchmark() {
    let source = std::env::var_os("SKWD_BENCH_VIDEO")
        .map(std::path::PathBuf::from)
        .expect("set SKWD_BENCH_VIDEO to a representative video");
    let dir = tempfile::tempdir().unwrap();
    let thumb = dir.path().join("video-thumbs/benchmark.webp");
    let small = dir.path().join("thumbs-sm/benchmark.webp");
    let started = std::time::Instant::now();
    let cpu_started = process_cpu_seconds();
    let memory_before = skwd_log::proc::mem_breakdown();
    let result = generate_video_thumbs(&source, &thumb, &small, 1).unwrap();
    let memory_after = skwd_log::proc::mem_breakdown();
    eprintln!(
        "scanner-video-benchmark source={}x{} elapsed_ms={} cpu_ms={:.1} rss_kb={} pss_kb={} rss_delta_kb={} pss_delta_kb={} thumb_bytes={} small_bytes={}",
        result.width,
        result.height,
        started.elapsed().as_millis(),
        (process_cpu_seconds() - cpu_started) * 1_000.0,
        memory_after.rss_kb,
        memory_after.pss_kb,
        memory_after.rss_kb.saturating_sub(memory_before.rss_kb),
        memory_after.pss_kb.saturating_sub(memory_before.pss_kb),
        std::fs::metadata(thumb).unwrap().len(),
        std::fs::metadata(small).unwrap().len(),
    );
}

fn process_cpu_seconds() -> f64 {
    let mut time = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut time) };
    time.tv_sec as f64 + time.tv_nsec as f64 / 1_000_000_000.0
}

#[test]
#[ignore = "manual scanner quality benchmark; set SKWD_BENCH_VIDEO"]
fn scanner_video_quality_benchmark() {
    let source = std::env::var_os("SKWD_BENCH_VIDEO")
        .map(std::path::PathBuf::from)
        .expect("set SKWD_BENCH_VIDEO to a representative video");
    let dir = tempfile::tempdir().unwrap();
    let legacy = dir.path().join("legacy.webp");
    let legacy_small = dir.path().join("legacy-small.webp");
    write_thumbs(decode_video_frame(&source, 1).unwrap(), &legacy, &legacy_small).unwrap();
    let direct = dir.path().join("direct.webp");
    let direct_small = dir.path().join("direct-small.webp");
    generate_video_thumbs(&source, &direct, &direct_small, 1).unwrap();

    let legacy = image::open(legacy).unwrap().to_rgb8();
    let direct = image::open(direct).unwrap().to_rgb8();
    let squared_error: f64 = legacy
        .as_raw()
        .iter()
        .zip(direct.as_raw())
        .map(|(left, right)| {
            let delta = f64::from(*left) - f64::from(*right);
            delta * delta
        })
        .sum();
    let mse = squared_error / legacy.as_raw().len() as f64;
    let psnr = if mse == 0.0 { f64::INFINITY } else { 10.0 * (255.0f64.powi(2) / mse).log10() };
    eprintln!("scanner-video-quality psnr_db={psnr:.2} mse={mse:.3}");
    assert!(psnr >= 30.0, "{psnr:.2} dB");
}

#[test]
#[ignore = "manual scanner memory benchmark; set SKWD_BENCH_IMAGE"]
fn scanner_image_benchmark() {
    let source = std::env::var_os("SKWD_BENCH_IMAGE")
        .map(std::path::PathBuf::from)
        .expect("set SKWD_BENCH_IMAGE to a representative image");
    let dir = tempfile::tempdir().unwrap();
    let thumb = dir.path().join("image-thumbs/benchmark.webp");
    let small = dir.path().join("thumbs-sm/benchmark.webp");
    let started = std::time::Instant::now();
    let result = generate_image_thumbs(&source, &thumb, &small).unwrap();
    eprintln!(
        "scanner-image-benchmark source={}x{} elapsed_ms={} thumb_bytes={} small_bytes={}",
        result.width,
        result.height,
        started.elapsed().as_millis(),
        std::fs::metadata(thumb).unwrap().len(),
        std::fs::metadata(small).unwrap().len(),
    );
}

#[test]
fn preview_animated_webp() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("clip.y4m");
    write_y4m(&src, 160, 120, &[(180, 75)]);
    let dest = dir.path().join("previews/clip.webp");
    let frames = generate_video_preview(&src, &dest).unwrap();
    assert!((2..=PREVIEW_MAX_FRAMES).contains(&frames));
    let data = std::fs::read(&dest).unwrap();
    assert!(data.len() > 12);
    assert_eq!(&data[0..4], b"RIFF");
    assert_eq!(&data[8..12], b"WEBP");
    assert!(
        !dir.path()
            .join("previews")
            .read_dir()
            .unwrap()
            .any(|ent| { ent.unwrap().file_name().to_string_lossy().contains(".tmp.") }),
    );
}

#[test]
fn preview_rejects_single_frame() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("one.y4m");
    write_y4m(&src, 160, 120, &[(180, 1)]);
    let dest = dir.path().join("one.webp");
    assert!(generate_video_preview(&src, &dest).is_err());
    assert!(!dest.exists());
}

struct OneFrameSink {
    data: Vec<u8>,
    limit: usize,
}

impl std::io::Write for OneFrameSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.data.len() >= self.limit {
            return Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe));
        }
        self.data.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn stream_stops_on_closed_pipe() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("clip.y4m");
    write_y4m(&src, 160, 120, &[(200, 5)]);
    let (w, h) = (32u32, 18u32);
    let frame_bytes = (w * h * 4) as usize;
    let mut sink = OneFrameSink { data: Vec::new(), limit: frame_bytes };
    stream_video_frames(&src, w, h, &mut sink).unwrap();
    assert_eq!(sink.data.len(), frame_bytes);
    let px = &sink.data[0..4];
    assert!(px[0] > 150 && px[1] > 150 && px[2] > 150);
    assert_eq!(px[3], 255);
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn png_header_only(w: u32, h: u32) -> Vec<u8> {
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(b"IHDR");
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    out.extend_from_slice(&13u32.to_be_bytes());
    out.extend_from_slice(&ihdr);
    out.extend_from_slice(&crc32(&ihdr).to_be_bytes());
    out
}

#[test]
fn oversized_header_rejected() {
    let bomb = png_header_only(60_000, 60_000);
    let err = load_from_memory_limited(&bomb).expect_err("oversized header rejected");
    let msg = format!("{err:#}").to_ascii_lowercase();
    assert!(msg.contains("limit") || msg.contains("dimension"), "got {msg}");
}

#[test]
fn decodes_above_four_k() {
    let mut encoded = std::io::Cursor::new(Vec::new());
    RgbImage::new(4000, 2250).write_to(&mut encoded, image::ImageFormat::Png).unwrap();
    let image = encoded.into_inner();
    let decoded = load_from_memory_limited(&image).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (4000, 2250));
}

#[test]
fn decode_limits() {
    let mut buf = std::io::Cursor::new(Vec::new());
    RgbImage::from_pixel(64, 64, Rgb([120, 60, 200]))
        .write_to(&mut buf, image::ImageFormat::Png)
        .unwrap();
    let png = buf.into_inner();
    assert!(load_from_memory_limited(&png).is_ok());
    let mut tiny = image::Limits::default();
    tiny.max_image_width = Some(16);
    tiny.max_image_height = Some(16);
    assert!(load_memory_with_limits(&png, tiny).is_err());
}

#[test]
fn disk_full_error_kinds() {
    for kind in [std::io::ErrorKind::StorageFull, std::io::ErrorKind::QuotaExceeded] {
        let error = anyhow::Error::from(std::io::Error::from(kind)).context("write thumb");
        assert!(is_disk_full(&error));
    }
    let other = anyhow::Error::from(std::io::Error::from_raw_os_error(13)).context("write");
    assert!(!is_disk_full(&other));
    assert!(!is_disk_full(&anyhow!("plain decode error")));
}

#[cfg(unix)]
#[test]
fn disk_full_unix_errors() {
    for errno in [libc::ENOSPC, libc::EDQUOT] {
        let error = anyhow::Error::from(std::io::Error::from_raw_os_error(errno));
        assert!(is_disk_full(&error));
    }
}

#[test]
fn extract_frame_decodable() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("clip.y4m");
    write_y4m(&src, 160, 120, &[(180, 30)]);
    let dest = dir.path().join("frames/first.png");
    extract_frame_to(&src, &dest, 0).unwrap();
    let img = image::open(&dest).unwrap();
    assert_eq!((img.width(), img.height()), (160, 120));
}
