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
    let buf = encode_near_mips(&full);
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
    write_blocks(&img, &near, &far).unwrap();
    let nbytes = std::fs::read(&near).unwrap();
    assert_eq!(&nbytes[0..4], SKB_MAGIC);
    assert!(nbytes.len() > near_size(NEAR_W, NEAR_H));
    assert_eq!(std::fs::metadata(&far).unwrap().len() as usize, far_size(FAR_W, FAR_H));
}
