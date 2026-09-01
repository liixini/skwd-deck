#![cfg(test)]

use super::*;

#[test]
fn parse_search_basic() {
    let json = r#"{
        "total_results": 45,
        "page": 2,
        "per_page": 15,
        "next_page": "https://api.pexels.com/v1/search?page=3",
        "photos": [
            {
                "id": 1181772,
                "width": 3840,
                "height": 2160,
                "url": "https://www.pexels.com/photo/1181772/",
                "photographer": "Ada Lovelace",
                "photographer_url": "https://www.pexels.com/@ada",
                "alt": "misty forest",
                "src": {
                    "original": "https://images.pexels.com/photos/1181772/orig.jpg",
                    "large2x": "https://images.pexels.com/photos/1181772/large2x.jpg",
                    "large": "https://images.pexels.com/photos/1181772/large.jpg",
                    "medium": "https://images.pexels.com/photos/1181772/medium.jpg"
                }
            },
            {"id": 2, "width": 100, "height": 100, "src": {"medium": "https://images.pexels.com/m.jpg"}}
        ]
    }"#;
    let page = parse_search(json, 2, 15).unwrap();
    assert_eq!(page.results.len(), 2);
    assert_eq!(page.current_page, 2);
    assert_eq!(page.last_page, 3);

    let first = &page.results[0];
    assert_eq!(first.id, "1181772");
    assert_eq!(first.full_url, "https://images.pexels.com/photos/1181772/orig.jpg");
    assert_eq!(first.thumb_url, "https://images.pexels.com/photos/1181772/large.jpg");
    assert_eq!(first.resolution, "3840x2160");
    assert_eq!(first.title, "misty forest");
    let attr = first.attribution.as_ref().expect("pexels requires photographer credit");
    assert_eq!(attr.text, "Photo by Ada Lovelace on Pexels");
    assert_eq!(attr.link, "https://www.pexels.com/photo/1181772/");

    let second = &page.results[1];
    assert_eq!(second.full_url, "https://images.pexels.com/m.jpg");
    assert!(second.attribution.is_none());
}

#[test]
fn curated_next_page() {
    let json = r#"{"page":1,"per_page":15,"next_page":"https://api.pexels.com/v1/curated?page=2","photos":[]}"#;
    let page = parse_search(json, 1, 15).unwrap();
    assert_eq!(page.last_page, 2);

    let last = r#"{"page":9,"per_page":15,"photos":[]}"#;
    assert_eq!(parse_search(last, 9, 15).unwrap().last_page, 9);
}

#[test]
fn api_error_surfaced() {
    assert!(parse_search(r#"{"error":"Not authorized"}"#, 1, 15).is_err());
}
