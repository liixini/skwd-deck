use super::{probe_mips, validate_mips, validate_probe};
use skwd_wall_core::blocks::{Mip, NEAR_BLOCK_BYTES, RGBA_CHANNELS, mip_shapes, near_bc7};

fn mips() -> Vec<Mip> {
    mip_shapes()
        .map(|(width, height)| Mip {
            width,
            height,
            rgba: vec![0; width as usize * height as usize * RGBA_CHANNELS],
        })
        .collect()
}

#[test]
fn validates_payload_size() {
    assert_eq!(validate_mips(&mips()).unwrap(), 307_520);
}

#[test]
fn rejects_bad_mips() {
    let mut levels = mips();
    levels[2].rgba.pop();
    assert!(validate_mips(&levels).is_err());
    levels[2].rgba.push(0);
    levels[2].height -= 1;
    assert!(validate_mips(&levels).is_err());
    levels.pop();
    assert!(validate_mips(&levels).is_err());
}

#[test]
fn accepts_reference_probe() {
    let blocks: Vec<_> =
        probe_mips().iter().flat_map(|mip| near_bc7(&mip.rgba, mip.width, mip.height)).collect();
    assert!(validate_probe(&blocks).is_ok());
}

#[test]
fn rejects_bad_probe() {
    assert!(validate_probe(&vec![0; NEAR_BLOCK_BYTES]).is_err());
    assert!(validate_probe(&[]).is_err());
}
