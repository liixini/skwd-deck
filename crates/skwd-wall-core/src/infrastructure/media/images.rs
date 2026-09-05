use std::path::Path;

use anyhow::Context;
use image::{DynamicImage, imageops::FilterType};

pub use wall_geom::cover_dims;

pub const THUMB_W: u32 = 640;
pub const THUMB_H: u32 = 360;
pub const SMALL_W: u32 = 240;
pub const SMALL_H: u32 = 135;

pub const FULL_QUALITY: f32 = 90.0;
pub const SMALL_QUALITY: f32 = 82.0;

const MIN_CHROMATIC_PCT: u64 = 5;

pub(super) fn decode_limits() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(crate::domain::media_limits::IMAGE_MAX_EDGE);
    limits.max_image_height = Some(crate::domain::media_limits::IMAGE_MAX_EDGE);
    limits.max_alloc = Some(crate::domain::media_limits::IMAGE_MAX_DECODE_ALLOC);
    limits
}

fn decode_reader<R: std::io::BufRead + std::io::Seek>(
    reader: image::ImageReader<R>,
    limits: image::Limits,
) -> anyhow::Result<DynamicImage> {
    let mut reader = reader;
    reader.limits(limits);
    Ok(DynamicImage::from_decoder(reader.into_decoder()?)?)
}

fn open_with_limits(path: &Path, limits: image::Limits) -> anyhow::Result<DynamicImage> {
    let reader = image::ImageReader::open(path)?.with_guessed_format()?;
    decode_reader(reader, limits)
}

pub(super) fn load_memory_with_limits(
    bytes: &[u8],
    limits: image::Limits,
) -> anyhow::Result<DynamicImage> {
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    decode_reader(reader, limits)
}

pub fn open_limited(path: &Path) -> anyhow::Result<DynamicImage> {
    open_with_limits(path, decode_limits())
}

pub fn load_from_memory_limited(bytes: &[u8]) -> anyhow::Result<DynamicImage> {
    load_memory_with_limits(bytes, decode_limits())
}

#[must_use]
pub fn is_disk_full(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        cause.downcast_ref::<std::io::Error>().is_some_and(|io| {
            matches!(io.kind(), std::io::ErrorKind::StorageFull | std::io::ErrorKind::QuotaExceeded)
        })
    })
}

pub struct ThumbResult {
    pub hue: u16,
    pub sat: u16,
    pub richness: u16,
    pub width: u32,
    pub height: u32,
    pub duration_ms: i64,
}

pub fn generate_placeholder_thumbs(thumb: &Path, small: &Path) -> anyhow::Result<ThumbResult> {
    let image =
        DynamicImage::ImageRgb8(image::RgbImage::from_pixel(16, 9, image::Rgb([48, 48, 48])));
    let mut result = write_thumbs(image, thumb, small)?;
    result.width = 0;
    result.height = 0;
    Ok(result)
}

fn encode_webp(img: &DynamicImage, dest: &Path, quality: f32) -> anyhow::Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let rgb = img.to_rgb8();
    let (w, h) = (rgb.width(), rgb.height());
    let encoded = webp::Encoder::from_rgb(rgb.as_raw(), w, h).encode(quality);
    crate::paths::atomic_write(dest, &encoded)
        .with_context(|| format!("write {}", dest.display()))?;
    Ok(())
}

pub fn preshrink_dims(w: u32, h: u32, cover_w: u32, cover_h: u32) -> Option<(u32, u32)> {
    if w == 0 || h == 0 {
        return None;
    }
    let scale =
        f64::max(f64::from(cover_w) * 2.0 / f64::from(w), f64::from(cover_h) * 2.0 / f64::from(h));
    if scale >= 1.0 {
        return None;
    }
    Some((
        ((f64::from(w) * scale).ceil() as u32).max(1),
        ((f64::from(h) * scale).ceil() as u32).max(1),
    ))
}

pub(super) fn write_thumbs(
    img: DynamicImage,
    thumb_path: &Path,
    thumb_sm_path: &Path,
) -> anyhow::Result<ThumbResult> {
    let (width, height) = (img.width(), img.height());
    let base = if let Some((tw, th)) = preshrink_dims(width, height, THUMB_W, THUMB_H) {
        let shrunk = img.thumbnail(tw, th);
        drop(img);
        shrunk
    } else {
        img
    };
    let full = base.resize_to_fill(THUMB_W, THUMB_H, FilterType::Lanczos3);
    drop(base);
    encode_webp(&full, thumb_path, FULL_QUALITY)?;
    let small = full.resize_to_fill(SMALL_W, SMALL_H, FilterType::Lanczos3);
    encode_webp(&small, thumb_sm_path, SMALL_QUALITY)?;
    let (near_dest, far_dest) = crate::blocks::dests_for(&thumb_path.to_string_lossy());
    if let Err(err) = crate::blocks::write_blocks(&full, &near_dest, &far_dest) {
        log::warn!("block bake failed for {}: {err}", thumb_path.display());
    }
    let (hue, sat, richness) = extract_hue_sat(&full);
    Ok(ThumbResult { hue, sat, richness, width, height, duration_ms: 0 })
}

pub fn generate_image_thumbs(
    src: &Path,
    thumb_path: &Path,
    thumb_sm_path: &Path,
) -> anyhow::Result<ThumbResult> {
    let img = open_limited(src).with_context(|| format!("decode {}", src.display()))?;
    write_thumbs(img, thumb_path, thumb_sm_path)
}

pub fn image_dimensions(src: &Path) -> Option<(u32, u32)> {
    image::image_dimensions(src).ok()
}

pub fn extract_colors_from(path: &Path) -> Option<(u16, u16, u16)> {
    let img = open_limited(path).ok()?;
    Some(extract_hue_sat(&img))
}

pub fn load_rgba(path: &Path, max_edge: u32) -> Option<Vec<u8>> {
    load_rgba_sized(path, max_edge).map(|(rgba, _, _)| rgba)
}

pub fn load_rgba_sized(path: &Path, max_edge: u32) -> Option<(Vec<u8>, usize, usize)> {
    let mut img = open_limited(path).ok()?;
    if max_edge > 0 && img.width().max(img.height()) > max_edge {
        img = img.resize(max_edge, max_edge, FilterType::Triangle);
    }
    let (width, height) = (img.width() as usize, img.height() as usize);
    Some((img.to_rgba8().into_raw(), width, height))
}

pub fn webp_from_bytes(
    bytes: &[u8],
    dest: &Path,
    quality: f32,
    max_edge: u32,
) -> anyhow::Result<()> {
    let mut img = load_from_memory_limited(bytes).context("decode remote image")?;
    if max_edge > 0 && img.width().max(img.height()) > max_edge {
        img = img.resize(max_edge, max_edge, FilterType::Triangle);
    }
    encode_webp(&img, dest, quality)
}

pub fn png_bytes(path: &Path, max_edge: u32) -> anyhow::Result<Vec<u8>> {
    let mut img = open_limited(path).with_context(|| format!("decode {}", path.display()))?;
    if max_edge > 0 && img.width().max(img.height()) > max_edge {
        img = img.resize(max_edge, max_edge, FilterType::Triangle);
    }
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).context("encode png")?;
    Ok(out.into_inner())
}

#[must_use]
pub fn extract_hue_sat(img: &DynamicImage) -> (u16, u16, u16) {
    let rgba = img.to_rgba8();
    let mut counts = [0u64; 13];
    let mut meaningful = 0u64;

    for px in rgba.pixels() {
        if let Some(bucket) = pixel_bucket(px[0], px[1], px[2]) {
            counts[bucket] += 1;
            meaningful += 1;
        }
    }

    if meaningful == 0 {
        return (0, 0, 0);
    }

    let (mut best_idx, mut best_count) = (0usize, 0u64);
    for (idx, &cnt) in counts.iter().enumerate().take(12) {
        if cnt > best_count {
            best_count = cnt;
            best_idx = idx;
        }
    }

    let chromatic_mass: u64 = counts[..12].iter().sum();
    let richness = richness_of(&counts, chromatic_mass);

    if chromatic_mass * 100 < meaningful * MIN_CHROMATIC_PCT {
        return (0, 0, richness);
    }

    let coverage = ((best_count as f64 / meaningful as f64) * 100.0).round() as u16;

    let hue_for_bucket: u16 = match best_idx {
        0 => 10,
        10 => 307,
        11 => 337,
        idx => 25 + (idx as u16 - 1) * 30 + 15,
    };

    (hue_for_bucket, coverage.clamp(10, 100), richness)
}

fn pixel_bucket(r: u8, g: u8, b: u8) -> Option<usize> {
    let r = f64::from(r) / 255.0;
    let g = f64::from(g) / 255.0;
    let b = f64::from(b) / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let lightness = f64::midpoint(max, min);

    if !(0.06..=0.94).contains(&lightness) {
        return None;
    }

    let sat =
        if delta < 1e-6 { 0.0 } else { delta / (1.0 - (2.0f64).mul_add(lightness, -1.0).abs()) };

    if sat < 0.18 {
        return Some(12);
    }

    let hue = if (max - r).abs() < 1e-6 {
        60.0 * (((g - b) / delta) % 6.0)
    } else if (max - g).abs() < 1e-6 {
        60.0f64.mul_add((b - r) / delta, 120.0)
    } else {
        60.0f64.mul_add((r - g) / delta, 240.0)
    };
    let hue = if hue < 0.0 { hue + 360.0 } else { hue };
    Some(hue_to_bucket_idx((hue.round() as u16) % 360))
}

fn richness_of(counts: &[u64; 13], chromatic_mass: u64) -> u16 {
    if chromatic_mass == 0 {
        return 0;
    }
    let total = chromatic_mass as f64;
    let mut sumsq = 0.0_f64;
    for &cnt in &counts[..12] {
        if cnt == 0 {
            continue;
        }
        let frac = cnt as f64 / total;
        sumsq += frac * frac;
    }
    if sumsq > 0.0 { ((1.0 / sumsq) * 100.0).round().clamp(0.0, 1500.0) as u16 } else { 0 }
}

#[must_use]
pub fn hue_bucket(hue: u16, sat: u16) -> u16 {
    if sat < 10 {
        return 99;
    }
    hue_to_bucket_idx(hue) as u16
}

fn hue_to_bucket_idx(hue: u16) -> usize {
    if !(25..355).contains(&hue) {
        return 0;
    }
    if hue >= 320 {
        return 11;
    }
    if hue >= 295 {
        return 10;
    }
    ((hue - 25) / 30 + 1) as usize
}
