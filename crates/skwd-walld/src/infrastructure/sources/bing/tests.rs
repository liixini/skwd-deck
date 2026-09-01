#![cfg(test)]

use super::*;

const SAMPLE: &str = r#"{
        "images": [
            {
                "startdate": "20260709",
                "urlbase": "/th?id=OHR.LakeBled_EN-US1234567890",
                "url": "/th?id=OHR.LakeBled_EN-US1234567890_1920x1080.jpg&rf=x&pid=hp",
                "copyright": "Lake Bled, Slovenia (© Photographer/Getty)",
                "copyrightlink": "https://www.bing.com/search?q=lake+bled",
                "title": "A storybook lake"
            },
            {
                "startdate": "20260708",
                "urlbase": "/th?id=OHR.Desert_EN-US9876543210",
                "url": "/th?id=OHR.Desert_EN-US9876543210_1920x1080.jpg",
                "copyright": "Sahara (© X)",
                "copyrightlink": "https://example.com",
                "title": "Dunes"
            }
        ]
    }"#;

#[test]
fn parse_search_uhd() {
    let page = parse_search(SAMPLE).unwrap();
    assert_eq!(page.last_page, 1);
    assert_eq!(page.next_cursor, None);
    assert_eq!(page.results.len(), 2);
    let first = &page.results[0];
    assert_eq!(first.id, "OHR.LakeBled_EN-US1234567890");
    assert_eq!(first.full_url, "https://www.bing.com/th?id=OHR.LakeBled_EN-US1234567890_UHD.jpg");
    assert_eq!(
        first.thumb_url,
        "https://www.bing.com/th?id=OHR.LakeBled_EN-US1234567890_400x240.jpg"
    );
    assert_eq!(first.resolution, "3840x2160");
    assert_eq!(first.title, "A storybook lake");
    let attr = first.attribution.as_ref().unwrap();
    assert!(attr.text.contains("Lake Bled"));
    assert_eq!(attr.link, "https://www.bing.com/search?q=lake+bled");
}

#[test]
fn empty_and_junk() {
    assert_eq!(parse_search(r#"{"images":[]}"#).unwrap().results.len(), 0);
    assert_eq!(parse_search("{}").unwrap().results.len(), 0);
    assert!(parse_search("not json at all").is_err());
}
