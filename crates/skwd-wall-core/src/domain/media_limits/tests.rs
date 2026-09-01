use super::*;

#[test]
fn allows_4k_orientations() {
    assert!(video_dimensions_allowed(3840, 2160));
    assert!(video_dimensions_allowed(2160, 3840));
}

#[test]
fn rejects_oversize_and_empty() {
    assert!(!video_dimensions_allowed(4000, 2250));
    assert!(!video_dimensions_allowed(8193, 1));
    assert!(!video_dimensions_allowed(0, 2160));
}
