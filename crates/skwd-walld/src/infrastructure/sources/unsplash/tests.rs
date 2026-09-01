#![cfg(test)]

use super::*;

const SAMPLE: &str = r#"{
        "total": 133,
        "total_pages": 14,
        "results": [
            {
                "id": "abc123",
                "width": 4000,
                "height": 3000,
                "urls": {
                    "raw": "https://images.unsplash.com/photo-1?raw",
                    "full": "https://images.unsplash.com/photo-1?full",
                    "small": "https://images.unsplash.com/photo-1?small",
                    "thumb": "https://images.unsplash.com/photo-1?thumb"
                },
                "links": {
                    "download_location": "https://api.unsplash.com/photos/abc123/download?ixid=xyz"
                },
                "user": {
                    "name": "Jane Doe",
                    "links": { "html": "https://unsplash.com/@jane" }
                }
            }
        ]
    }"#;

#[test]
fn parse_search_basic() {
    let page = parse_search(SAMPLE, 2).unwrap();
    assert_eq!(page.last_page, 14);
    assert_eq!(page.current_page, 2);
    assert_eq!(page.next_cursor, None);
    let first = &page.results[0];
    assert_eq!(first.id, "abc123");
    assert_eq!(first.full_url, "https://images.unsplash.com/photo-1?full");
    assert_eq!(first.thumb_url, "https://images.unsplash.com/photo-1?small");
    assert_eq!(first.resolution, "4000x3000");
    assert_eq!(first.track_url, "https://api.unsplash.com/photos/abc123/download?ixid=xyz");
}

#[test]
fn attribution_utm() {
    let page = parse_search(SAMPLE, 1).unwrap();
    let attr = page.results[0].attribution.as_ref().unwrap();
    assert_eq!(attr.text, "Photo by Jane Doe on Unsplash");
    assert_eq!(attr.link, "https://unsplash.com/@jane?utm_source=skwd-wall&utm_medium=referral");
}

#[test]
fn referral_amp_join() {
    assert_eq!(
        referral_link("https://unsplash.com/@x?foo=1"),
        "https://unsplash.com/@x?foo=1&utm_source=skwd-wall&utm_medium=referral"
    );
    assert_eq!(referral_link(""), "");
}

#[test]
fn api_errors_surfaced() {
    assert!(parse_search(r#"{"errors":["OAuth error: invalid"]}"#, 1).is_err());
}

#[test]
fn empty_and_junk() {
    assert_eq!(parse_search(r#"{"results":[]}"#, 1).unwrap().results.len(), 0);
    assert!(parse_search("nope", 1).is_err());
}
