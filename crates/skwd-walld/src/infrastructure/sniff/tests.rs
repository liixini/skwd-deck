#![cfg(test)]

use super::{Kind, check_file, describe, detect, verify};

#[test]
fn detect_images() {
    assert_eq!(detect(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 0, 0, 0, 0, 0, 0, 0]), Some(Kind::Image));
    assert_eq!(detect(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"), Some(Kind::Image));
    assert_eq!(detect(b"GIF89a______"), Some(Kind::Image));
    assert_eq!(detect(b"GIF87a______"), Some(Kind::Image));
    assert_eq!(detect(b"RIFF\x10\x00\x00\x00WEBPVP8 "), Some(Kind::Image));
    assert_eq!(detect(b"BM\x36\x00\x00\x00\x00\x00\x00\x00\x00\x00"), Some(Kind::Image));
    assert_eq!(detect(b"\x00\x00\x00\x1cftypavif\x00\x00"), Some(Kind::Image));
    assert_eq!(detect(b"\x00\x00\x00\x18ftypheic\x00\x00"), Some(Kind::Image));
}

#[test]
fn detect_videos() {
    assert_eq!(detect(b"\x00\x00\x00\x20ftypisom\x00\x00"), Some(Kind::Video));
    assert_eq!(detect(b"\x00\x00\x00\x20ftypmp42\x00\x00"), Some(Kind::Video));
    assert_eq!(detect(b"\x1a\x45\xdf\xa3\x01\x00\x00\x00\x00\x00\x00\x00"), Some(Kind::Video));
    assert_eq!(detect(b"RIFF\x10\x00\x00\x00AVI LIST"), Some(Kind::Video));
}

#[test]
fn detect_rejects_junk() {
    assert_eq!(detect(b"<!DOCTYPE html><html>"), None);
    assert_eq!(detect(b"{\"error\":\"rate limited\"}"), None);
    assert_eq!(detect(b""), None);
    assert_eq!(detect(b"\xFF\xD8"), None);
    assert_eq!(detect(b"RIFF\x10\x00\x00\x00WAVEfmt "), None);
}

#[test]
fn describe_masquerades() {
    assert_eq!(describe(b"  <html><body>404"), "an HTML/text page");
    assert_eq!(describe(b"{\"error\": true}"), "a JSON document");
    assert_eq!(describe(b""), "an empty file");
    assert!(describe(&[0xDE, 0xAD]).contains("de ad"));
}

#[test]
fn verify_kind_error() {
    assert!(verify(b"\x89PNG\r\n\x1a\n\0\0\0\r", Kind::Image).is_ok());
    let cross = verify(b"\x00\x00\x00\x20ftypisom\x00\x00", Kind::Image).unwrap_err();
    assert!(cross.to_string().contains("video"));
    let html = verify(b"<html>oops</html>", Kind::Image).unwrap_err();
    assert!(html.to_string().contains("HTML"));
}

#[test]
fn check_file_head() {
    let dir = tempfile::tempdir().unwrap();
    let good = dir.path().join("a.png");
    std::fs::write(&good, b"\x89PNG\r\n\x1a\n\0\0\0\rIHDRxxxx").unwrap();
    assert!(check_file(&good, Kind::Image).is_ok());
    let bad = dir.path().join("b.jpg");
    std::fs::write(&bad, b"<!DOCTYPE html><html>nope</html>").unwrap();
    assert!(check_file(&bad, Kind::Image).is_err());
    let vid = dir.path().join("c.mp4");
    std::fs::write(&vid, b"\x00\x00\x00\x20ftypisom\x00\x00\x02\x00").unwrap();
    assert!(check_file(&vid, Kind::Video).is_ok());
    assert!(check_file(&vid, Kind::Image).is_err());
    assert!(check_file(&dir.path().join("missing"), Kind::Image).is_err());
}

use skwd_wall_core::xorshift64 as xs;

#[test]
fn sniff_fuzz() {
    let magics: [&[u8]; 6] = [
        b"\x89PNG\r\n\x1a\n",
        b"\xff\xd8\xff\xe0",
        b"RIFF\x00\x00\x00\x00WEBP",
        b"\x00\x00\x00\x20ftypisom",
        b"\x1aE\xdf\xa3",
        b"GIF89a",
    ];
    let mut seed = 0x5eed_beef_0002u64;
    for _ in 0..8000 {
        let len = (xs(&mut seed) % 40) as usize;
        let mut buf: Vec<u8> = (0..len).map(|_| (xs(&mut seed) & 0xff) as u8).collect();
        if xs(&mut seed).is_multiple_of(3) && !buf.is_empty() {
            let magic = magics[(xs(&mut seed) % magics.len() as u64) as usize];
            let take = (xs(&mut seed) as usize % (magic.len() + 1)).min(buf.len());
            buf[..take].copy_from_slice(&magic[..take]);
        }
        let kind = detect(&buf);
        let _ = describe(&buf);
        assert!(verify(&buf, Kind::Image).is_ok() == matches!(kind, Some(Kind::Image)), "{buf:?}");
        let _ = verify(&buf, Kind::Video);
    }
}
