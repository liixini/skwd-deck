use std::path::Path;

use anyhow::Context;
use image::{DynamicImage, imageops::FilterType};

pub const NEAR_W: u32 = 640;
pub const NEAR_H: u32 = 360;
pub const FAR_W: u32 = 160;
pub const FAR_H: u32 = 88;

pub const NEAR_EXT: &str = "bc7";
pub const FAR_EXT: &str = "bc1";

pub const RGBA_CHANNELS: usize = 4;
pub const BLOCK_EDGE: u32 = 4;

pub struct Mip {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

use super::Encoder;
const ALPHA_CHANNEL: usize = 3;
const BC1_ALPHA_CUTOFF: u8 = 128;

pub fn near_size(w: u32, h: u32) -> usize {
    (w / 4) as usize * (h / 4) as usize * 16
}

pub fn far_size(w: u32, h: u32) -> usize {
    (w / 4) as usize * (h / 4) as usize * 8
}

pub use crate::paths::thumbnail_blocks as dests_for;

pub const SKB_MAGIC: &[u8; 4] = b"SKB1";
pub const FMT_BC7: u8 = 0;
pub const NEAR_MIP_MIN: u32 = 16;

pub fn near_bc7(rgba: &[u8], w: u32, h: u32) -> Vec<u8> {
    let surface = intel_tex_2::RgbaSurface { data: rgba, width: w, height: h, stride: w * 4 };
    intel_tex_2::bc7::compress_blocks(&intel_tex_2::bc7::opaque_fast_settings(), &surface)
}

fn align4(x: u32) -> u32 {
    x.div_ceil(BLOCK_EDGE) * BLOCK_EDGE
}

pub fn mip_levels(base_w: u32, base_h: u32) -> Vec<(u32, u32)> {
    let mut levels = Vec::new();
    let (mut w, mut h) = (base_w, base_h);
    loop {
        levels.push((w, h));
        let (nw, nh) = (w / 2, h / 2);
        if nw.min(nh) < NEAR_MIP_MIN {
            break;
        }
        w = nw;
        h = nh;
    }
    levels
}

fn pad_rgba(src: &[u8], w: u32, h: u32, aw: u32, ah: u32) -> Vec<u8> {
    let mut out = vec![0u8; (aw * ah * 4) as usize];
    for y in 0..ah {
        let sy = y.min(h - 1);
        for x in 0..aw {
            let sx = x.min(w - 1);
            let src_off = ((sy * w + sx) * 4) as usize;
            let dst_off = ((y * aw + x) * 4) as usize;
            out[dst_off..dst_off + 4].copy_from_slice(&src[src_off..src_off + 4]);
        }
    }
    out
}

pub fn mip_shapes() -> impl Iterator<Item = (u32, u32)> {
    mip_levels(NEAR_W, NEAR_H).into_iter().map(|(w, h)| (align4(w), align4(h)))
}

pub fn near_mips(full: &DynamicImage) -> Vec<Mip> {
    mip_levels(NEAR_W, NEAR_H)
        .into_iter()
        .enumerate()
        .map(|(idx, (w, h))| {
            let rgba = if idx == 0 {
                full.to_rgba8()
            } else {
                full.resize_exact(w, h, FilterType::Lanczos3).to_rgba8()
            };
            let (width, height) = (align4(w), align4(h));
            let rgba = if (width, height) == (w, h) {
                rgba.into_raw()
            } else {
                pad_rgba(rgba.as_raw(), w, h, width, height)
            };
            Mip { width, height, rgba }
        })
        .collect()
}

pub fn encode_near_mips(full: &DynamicImage, encoder: &Encoder) -> Vec<u8> {
    let levels = mip_levels(NEAR_W, NEAR_H);
    let mips = near_mips(full);
    let data = encoder.encode(&mips);
    let index: Vec<_> = levels
        .iter()
        .zip(&mips)
        .map(|(&(w, h), mip)| (w, h, near_size(mip.width, mip.height) as u32))
        .collect();
    let mut buf = Vec::with_capacity(12 + index.len() * 8 + data.len());
    buf.extend_from_slice(SKB_MAGIC);
    buf.push(FMT_BC7);
    buf.push(levels.len() as u8);
    buf.extend_from_slice(&(NEAR_W as u16).to_le_bytes());
    buf.extend_from_slice(&(NEAR_H as u16).to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes());
    for (w, h, len) in &index {
        buf.extend_from_slice(&(*w as u16).to_le_bytes());
        buf.extend_from_slice(&(*h as u16).to_le_bytes());
        buf.extend_from_slice(&len.to_le_bytes());
    }
    buf.extend_from_slice(&data);
    buf
}

pub fn far_bc1(rgba: &[u8], w: u32, h: u32) -> Vec<u8> {
    let bytes = (u64::from(w) * u64::from(h))
        .checked_mul(RGBA_CHANNELS as u64)
        .and_then(|bytes| usize::try_from(bytes).ok())
        .expect("BC1 dimensions overflow");
    assert_eq!(rgba.len(), bytes, "BC1 pixel count");
    if bytes == 0 {
        return Vec::new();
    }
    if (w, h) == (FAR_W, FAR_H)
        && rgba.chunks_exact(RGBA_CHANNELS).all(|pixel| pixel[ALPHA_CHANNEL] >= BC1_ALPHA_CUTOFF)
    {
        let surface = intel_tex_2::RgbaSurface {
            data: rgba,
            width: w,
            height: h,
            stride: w * RGBA_CHANNELS as u32,
        };
        return intel_tex_2::bc1::compress_blocks(&surface);
    }
    let (w, h) = (w as usize, h as usize);
    let params =
        texpresso::Params { algorithm: texpresso::Algorithm::ClusterFit, ..Default::default() };
    let mut out = vec![0u8; texpresso::Format::Bc1.compressed_size(w, h)];
    texpresso::Format::Bc1.compress(rgba, w, h, params, &mut out);
    out
}

fn write_atomic(dest: &Path, data: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    crate::paths::atomic_write(dest, data).with_context(|| format!("write {}", dest.display()))
}

pub fn write_blocks(
    full: &DynamicImage,
    near_dest: &Path,
    far_dest: &Path,
    encoder: &Encoder,
) -> anyhow::Result<()> {
    let near = if full.width() == NEAR_W && full.height() == NEAR_H {
        encode_near_mips(full, encoder)
    } else {
        encode_near_mips(&full.resize_to_fill(NEAR_W, NEAR_H, FilterType::Lanczos3), encoder)
    };
    write_atomic(near_dest, &near)?;
    let far_rgba = full.resize_to_fill(FAR_W, FAR_H, FilterType::Lanczos3).to_rgba8();
    write_atomic(far_dest, &far_bc1(far_rgba.as_raw(), FAR_W, FAR_H))?;
    Ok(())
}

#[path = "tests.rs"]
mod tests;
