#![cfg(test)]

use super::*;

#[test]
fn fmt_duration_clock() {
    assert_eq!(fmt_duration(0.0), "0:00");
    assert_eq!(fmt_duration(65.0), "1:05");
    assert_eq!(fmt_duration(201.0), "3:21");
    assert_eq!(fmt_duration(3661.0), "1:01:01");
    assert_eq!(fmt_duration(-5.0), "0:00");
}

#[test]
fn safe_id_gate() {
    assert!(safe_id("dQw4w9WgXcQ"));
    assert!(safe_id("a-b_c"));
    assert!(!safe_id(""));
    assert!(!safe_id("../../etc/passwd"));
    assert!(!safe_id("a b"));
    assert!(!safe_id("a&b"));
    assert!(!safe_id(&"x".repeat(25)));
}

#[test]
fn parse_search_ndjson() {
    let ndjson = concat!(
        r#"{"id":"dQw4w9WgXcQ","title":"4K Forest Loop","duration":201.0,"channel":"Ambient Co","thumbnails":[{"url":"https://i.ytimg.com/vi/dQw4w9WgXcQ/default.jpg","width":120,"height":90},{"url":"https://i.ytimg.com/vi/dQw4w9WgXcQ/maxres.jpg","width":1280,"height":720}]}"#,
        "\n",
        r#"{"id":"abc123","title":"No Thumbs","duration":65.0,"uploader":"Someone","thumbnails":[]}"#,
        "\n",
    );
    let page = parse_search(ndjson, 1, 2, 0);
    assert_eq!(page.results.len(), 2);

    let first = &page.results[0];
    assert_eq!(first.id, "dQw4w9WgXcQ");
    assert_eq!(first.full_url, "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
    assert_eq!(first.thumb_url, "https://i.ytimg.com/vi/dQw4w9WgXcQ/maxres.jpg");
    assert_eq!(first.title, "4K Forest Loop");
    assert_eq!(first.resolution, "3:21");
    assert_eq!(first.attribution.as_ref().unwrap().text, "Ambient Co");

    let second = &page.results[1];
    assert_eq!(second.thumb_url, "https://i.ytimg.com/vi/abc123/hqdefault.jpg");
    assert_eq!(second.attribution.as_ref().unwrap().text, "Someone");
}

#[test]
fn parse_search_junk() {
    let ndjson = concat!(
        r#"{"id":"live1","title":"24/7 Lofi Radio","live_status":"is_live"}"#,
        "\n",
        "not json at all\n",
        r#"{"title":"no id"}"#,
        "\n",
        r#"{"id":"ok1","title":"Fine"}"#,
        "\n",
    );
    let page = parse_search(ndjson, 1, 20, 0);
    assert_eq!(page.results.len(), 1);
    assert_eq!(page.results[0].id, "ok1");
}

#[test]
fn duration_filter() {
    assert!(keeps_duration(37251, 0));
    assert!(keeps_duration(0, 0));
    assert!(keeps_duration(180, 600));
    assert!(keeps_duration(600, 600));
    assert!(!keeps_duration(601, 600));
    assert!(!keeps_duration(37251, 600));
    assert!(!keeps_duration(0, 600));
}

#[test]
fn filter_fills_page() {
    use std::fmt::Write;
    let mut lines = String::new();
    for i in 0..8 {
        let _ = writeln!(lines, r#"{{"id":"long{i}","title":"10h loop","duration":37251.0}}"#);
        let _ = writeln!(lines, r#"{{"id":"short{i}","title":"3m loop","duration":180.0}}"#);
    }
    let page = parse_search(&lines, 1, 4, 600);
    assert_eq!(
        page.results.iter().map(|res| res.id.as_str()).collect::<Vec<_>>(),
        ["short0", "short1", "short2", "short3"]
    );
    assert_eq!(page.last_page, 2);
    let page2 = parse_search(&lines, 2, 4, 600);
    assert_eq!(
        page2.results.iter().map(|res| res.id.as_str()).collect::<Vec<_>>(),
        ["short4", "short5", "short6", "short7"]
    );
    assert_eq!(page2.last_page, 2);
}

#[test]
fn raw_size_oversample() {
    assert_eq!(raw_search_size(1, 24, 0), 25);
    assert_eq!(raw_search_size(2, 24, 0), 49);
    assert_eq!(raw_search_size(1, 24, 600), 97, "filter over-fetch");
    assert_eq!(raw_search_size(9, 24, 600), MAX_SEARCH);
}

#[test]
fn paging_while_full() {
    let more = concat!(r#"{"id":"a1"}"#, "\n", r#"{"id":"a2"}"#, "\n", r#"{"id":"a3"}"#, "\n");
    assert_eq!(parse_search(more, 1, 2, 0).last_page, 2);

    let exact = concat!(r#"{"id":"a1"}"#, "\n", r#"{"id":"a2"}"#, "\n");
    assert_eq!(parse_search(exact, 1, 2, 0).last_page, 1);

    let partial = concat!(r#"{"id":"a1"}"#, "\n");
    assert_eq!(parse_search(partial, 3, 2, 0).last_page, 3);
    assert_eq!(parse_search(partial, 3, 2, 0).current_page, 3);
}
