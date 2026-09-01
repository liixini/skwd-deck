#![cfg(test)]

use super::local_timestamp;

#[test]
fn timestamp_shape_and_millis() {
    let stamp = local_timestamp(1_700_000_000, 7);
    assert_eq!(stamp.len(), 23, "{stamp}");
    let bytes = stamp.as_bytes();
    assert_eq!(bytes[4], b'-');
    assert_eq!(bytes[7], b'-');
    assert_eq!(bytes[10], b' ');
    assert_eq!(bytes[13], b':');
    assert_eq!(bytes[16], b':');
    assert_eq!(bytes[19], b'.');
    assert!(stamp.ends_with(".007"), "{stamp}");
    assert!(stamp.starts_with("20"), "{stamp}");
}
