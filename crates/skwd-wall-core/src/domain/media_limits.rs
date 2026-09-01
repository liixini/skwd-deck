pub const IMAGE_MAX_EDGE: u32 = 16_384;
pub const IMAGE_MAX_DECODE_ALLOC: u64 = 512 * 1024 * 1024;
pub const VIDEO_MAX_PIXELS: u64 = 3840 * 2160;
pub const VIDEO_MAX_EDGE: u32 = 8192;

#[must_use]
pub fn video_dimensions_allowed(width: u32, height: u32) -> bool {
    width > 0
        && height > 0
        && width <= VIDEO_MAX_EDGE
        && height <= VIDEO_MAX_EDGE
        && u64::from(width).saturating_mul(u64::from(height)) <= VIDEO_MAX_PIXELS
}

#[cfg(test)]
#[path = "media_limits/tests.rs"]
mod tests;
