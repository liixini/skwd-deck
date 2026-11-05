#![cfg(test)]

use super::*;

#[test]
fn dests_by_kind() {
    let (near, far) = dests_for("/c/skwd-wall-v2/thumbs/a.webp");
    assert_eq!(near.to_str().unwrap(), "/c/skwd-wall-v2/blocks/a.bc7");
    assert_eq!(far.to_str().unwrap(), "/c/skwd-wall-v2/blocks/a.bc1");

    let (near, _) = dests_for("/c/skwd-wall-v2/video-thumbs/clip.webp");
    assert_eq!(near.to_str().unwrap(), "/c/skwd-wall-v2/blocks/vid--clip.bc7");

    let (_, far) = dests_for("/c/skwd-wall-v2/we-thumbs/123.webp");
    assert_eq!(far.to_str().unwrap(), "/c/skwd-wall-v2/blocks/we--123.bc1");
}

#[test]
fn block_sizes() {
    assert_eq!(near_size(NEAR_W, NEAR_H), 230_400);
    assert_eq!(far_size(FAR_W, FAR_H), 7_040);
}

#[test]
fn encoder_sizes() {
    let near = near_bc7(&vec![128u8; (NEAR_W * NEAR_H * 4) as usize], NEAR_W, NEAR_H);
    assert_eq!(near.len(), near_size(NEAR_W, NEAR_H));
    let far = far_bc1(&vec![128u8; (FAR_W * FAR_H * 4) as usize], FAR_W, FAR_H);
    assert_eq!(far.len(), far_size(FAR_W, FAR_H));
}

#[test]
fn far_decodes_opaque() {
    let source = image::RgbaImage::from_fn(FAR_W, FAR_H, |x, y| {
        image::Rgba([(x * 255 / FAR_W) as u8, (y * 255 / FAR_H) as u8, 80, 255])
    });
    let encoded = far_bc1(source.as_raw(), FAR_W, FAR_H);
    assert_eq!(encoded.len(), 7_040);

    let mut decoded = vec![0; source.as_raw().len()];
    texpresso::Format::Bc1.decompress(&encoded, FAR_W as usize, FAR_H as usize, &mut decoded);
    let mut error = 0_u64;
    for (original, pixel) in source.as_raw().chunks_exact(4).zip(decoded.chunks_exact(4)) {
        assert_eq!(pixel[3], 255);
        for channel in 0..3 {
            let difference = i32::from(original[channel]) - i32::from(pixel[channel]);
            error += difference.pow(2) as u64;
        }
    }
    let mean_error = error as f64 / f64::from(FAR_W * FAR_H * 3);
    assert!(mean_error < 10.0, "colour error {mean_error}");
}

#[test]
fn far_retains_cutout() {
    let source = image::RgbaImage::from_fn(FAR_W, FAR_H, |x, _| {
        image::Rgba([40, 120, 200, if x % 4 < 2 { 127 } else { 128 }])
    });
    let encoded = far_bc1(source.as_raw(), FAR_W, FAR_H);
    let mut decoded = [0; 64];
    bcdec_rs::bc1(&encoded[..8], &mut decoded, 16);
    for (x, pixel) in decoded.chunks_exact(4).enumerate() {
        assert_eq!(pixel[3], if x % 4 < 2 { 0 } else { 255 });
    }
}

#[test]
fn far_pads_edges() {
    let source = image::RgbaImage::from_pixel(6, 5, image::Rgba([160, 80, 40, 255]));
    let encoded = far_bc1(source.as_raw(), source.width(), source.height());
    assert_eq!(encoded.len(), texpresso::Format::Bc1.compressed_size(6, 5));
    let mut decoded = vec![0; source.as_raw().len()];
    texpresso::Format::Bc1.decompress(&encoded, 6, 5, &mut decoded);
    assert!(decoded.chunks_exact(4).all(|pixel| pixel[3] == 255));
}

#[test]
fn near_retains_detail() {
    let source = image::RgbaImage::from_fn(64, 64, |x, y| {
        image::Rgba([(x * 4) as u8, (y * 4) as u8, if x < y { 40 } else { 200 }, 255])
    });
    let encoded = near_bc7(source.as_raw(), source.width(), source.height());
    let mut squared_error = 0_u64;
    for (index, block) in encoded.chunks_exact(16).enumerate() {
        let mut decoded = [0; 64];
        bcdec_rs::bc7(block, &mut decoded, 16);
        let left = (index as u32 % 16) * 4;
        let top = (index as u32 / 16) * 4;
        for y in 0..4 {
            for x in 0..4 {
                let original = source.get_pixel(left + x, top + y);
                let offset = ((y * 4 + x) * 4) as usize;
                for channel in 0..3 {
                    let error = i32::from(original[channel]) - i32::from(decoded[offset + channel]);
                    squared_error += error.pow(2) as u64;
                }
                assert_eq!(decoded[offset + 3], 255);
            }
        }
    }
    let mean_error = squared_error as f64 / f64::from(source.width() * source.height() * 3);
    assert!(mean_error < 2.0, "colour error {mean_error}");
}

#[test]
fn mip_levels_halve() {
    let levels = mip_levels(NEAR_W, NEAR_H);
    assert_eq!(levels[0], (640, 360));
    assert_eq!(levels[1], (320, 180));
    assert!(levels.iter().all(|&(w, h)| w.min(h) >= NEAR_MIP_MIN));
    assert!(levels.len() >= 3);
}

#[test]
fn near_container_header() {
    let img = DynamicImage::ImageRgb8(image::RgbImage::new(800, 600));
    let full = img.resize_to_fill(NEAR_W, NEAR_H, FilterType::Lanczos3);
    let buf = encode_near_mips(&full, &crate::blocks::Encoder::default());
    assert_eq!(&buf[0..4], SKB_MAGIC);
    assert_eq!(buf[4], FMT_BC7);
    let levels = buf[5] as usize;
    assert_eq!(levels, mip_levels(NEAR_W, NEAR_H).len());
    let (w0, h0) = (u16::from_le_bytes([buf[6], buf[7]]), u16::from_le_bytes([buf[8], buf[9]]));
    assert_eq!((w0, h0), (NEAR_W as u16, NEAR_H as u16));
    let l0_len = u32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]) as usize;
    assert_eq!(l0_len, near_size(NEAR_W, NEAR_H));
    let total: usize = (0..levels)
        .map(|idx| {
            let off = 12 + idx * 8 + 4;
            u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]) as usize
        })
        .sum();
    assert_eq!(buf.len(), 12 + levels * 8 + total);
}

#[test]
fn write_blocks_outputs() {
    let dir = tempfile::tempdir().unwrap();
    let near = dir.path().join("blocks/a.bc7");
    let far = dir.path().join("blocks/a.bc1");
    let img = DynamicImage::ImageRgb8(image::RgbImage::new(800, 600));
    write_blocks(&img, &near, &far, &crate::blocks::Encoder::default()).unwrap();
    let nbytes = std::fs::read(&near).unwrap();
    assert_eq!(&nbytes[0..4], SKB_MAGIC);
    assert!(nbytes.len() > near_size(NEAR_W, NEAR_H));
    assert_eq!(std::fs::metadata(&far).unwrap().len() as usize, far_size(FAR_W, FAR_H));
}

#[test]
#[should_panic(expected = "BC1 pixel count")]
fn far_rejects_short_pixels() {
    far_bc1(&[255; 4], FAR_W, FAR_H);
}

#[test]
#[should_panic(expected = "BC1 pixel count")]
fn far_rejects_extra_pixels() {
    far_bc1(&vec![255; (FAR_W * FAR_H * 4) as usize + 1], FAR_W, FAR_H);
}

#[test]
fn far_accepts_empty_image() {
    assert!(far_bc1(&[], 0, 4).is_empty());
}

#[test]
fn high_depth_mips() {
    let full = DynamicImage::ImageRgb16(image::ImageBuffer::from_fn(NEAR_W, NEAR_H, |x, y| {
        image::Rgb([(x * 101 + y * 317) as u16, (x * 593) as u16, (y * 997) as u16])
    }));
    let quantized = DynamicImage::ImageRgba8(full.to_rgba8());
    let mut differs = false;
    for (mip, (w, h)) in near_mips(&full).iter().zip(mip_levels(NEAR_W, NEAR_H)) {
        let expected = full.resize_exact(w, h, FilterType::Lanczos3).to_rgba8();
        let early = quantized.resize_exact(w, h, FilterType::Lanczos3).to_rgba8();
        differs |= expected != early;
        assert_eq!(mip.rgba, pad_rgba(expected.as_raw(), w, h, mip.width, mip.height));
    }
    assert!(differs);
}
