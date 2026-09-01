#![cfg(test)]

use super::*;

#[test]
fn within_max_bounds() {
    assert!(within_max("1920x1080", "3840x2160"));
    assert!(within_max("3840x2160", "3840x2160"));
    assert!(!within_max("5120x2880", "3840x2160"));
    assert!(!within_max("3840x2161", "3840x2160"));
    assert!(!within_max("2560x1080", "1920x1080"));
}

#[test]
fn within_max_bad_ceiling() {
    assert!(within_max("99999x99999", ""));
    assert!(within_max("1920x1080", "garbage"));
    assert!(within_max("unknown", "1920x1080"));
    assert!(within_max("1920 X 1080", "1920x1080"));
}

#[test]
fn ext_from_url_pick() {
    assert_eq!(ext_from_url("https://w.wallhaven.cc/full/ab/wallhaven-abc.jpg"), "jpg");
    assert_eq!(ext_from_url("https://x/y/z.png"), "png");
    assert_eq!(ext_from_url("https://x/no-extension"), "jpg");
    assert_eq!(ext_from_url("https://x/a.b/weird"), "jpg");
}

#[test]
fn library_has_id() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("wallhaven-abc1.jpg"), b"x").unwrap();
    let root = dir.path().to_str().unwrap();
    assert!(library_path(root, "abc1").is_some());
    assert!(library_path(root, "zzz9").is_none());
}

#[test]
fn library_path_existing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wallhaven-abc1.png");
    std::fs::write(&path, b"x").unwrap();
    let root = dir.path().to_str().unwrap();
    assert_eq!(library_path(root, "abc1"), Some(path));
    assert_eq!(library_path(root, "zzz9"), None);
}

#[test]
fn library_ids_collect() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("wallhaven-abc1.jpg"), b"x").unwrap();
    std::fs::write(dir.path().join("wallhaven-d2e3.png"), b"x").unwrap();
    std::fs::write(dir.path().join("other.jpg"), b"x").unwrap();
    let ids = library_ids(dir.path().to_str().unwrap());
    assert!(ids.contains("abc1"));
    assert!(ids.contains("d2e3"));
    assert_eq!(ids.len(), 2);
}

#[test]
fn query_defaults() {
    let params = SearchParams::default();
    let pairs = query_pairs(&params);
    assert!(pairs.contains(&("categories", "111".into())));
    assert!(pairs.contains(&("purity", "100".into())));
    assert!(pairs.contains(&("sorting", "toplist".into())));
    assert!(pairs.contains(&("topRange", "1M".into())));
    assert!(pairs.contains(&("page", "1".into())));
    assert!(!pairs.iter().any(|(key, _)| *key == "q"));
}

#[test]
fn resolutions_beat_atleast() {
    let exact = SearchParams {
        atleast: "1920x1080".into(),
        resolutions: "1920x1080,3840x2160".into(),
        ratios: "16x9,16x10".into(),
        ..Default::default()
    };
    let pairs = query_pairs(&exact);
    assert!(pairs.contains(&("resolutions", "1920x1080,3840x2160".into())));
    assert!(!pairs.iter().any(|(key, _)| *key == "atleast"));
    assert!(pairs.contains(&("ratios", "16x9,16x10".into())));

    let min = SearchParams { atleast: "2560x1440".into(), ..Default::default() };
    let pairs = query_pairs(&min);
    assert!(pairs.contains(&("atleast", "2560x1440".into())));
    assert!(!pairs.iter().any(|(key, _)| *key == "resolutions"));
}

#[test]
fn query_non_toplist() {
    let params = SearchParams {
        query: "forest cabin".into(),
        sorting: "date_added".into(),
        ..Default::default()
    };
    let pairs = query_pairs(&params);
    assert!(pairs.contains(&("q", "forest cabin".into())));
    assert!(!pairs.iter().any(|(key, _)| *key == "topRange"));
}

#[test]
fn parse_search_basic() {
    let json = r#"{
            "data": [
                {"id":"abc1","path":"https://w.wallhaven.cc/full/ab/wallhaven-abc1.jpg",
                 "resolution":"3840x2160","file_size":1234567,"purity":"sfw","category":"general",
                 "thumbs":{"small":"https://th.wallhaven.cc/small/ab/abc1.jpg","large":"https://th/large.jpg"}},
                {"id":"def2","path":"https://w.wallhaven.cc/full/de/wallhaven-def2.png","resolution":"2560x1440","purity":"sketchy","category":"anime","thumbs":{"small":"","large":"https://th/large2.jpg"}}
            ],
            "meta": {"last_page": 42, "current_page": 3}
        }"#;
    let page = parse_search(json).unwrap();
    assert_eq!(page.results.len(), 2);
    assert_eq!(page.last_page, 42);
    assert_eq!(page.current_page, 3);
    assert_eq!(page.results[0].id, "abc1");
    assert_eq!(page.results[0].full_url, "https://w.wallhaven.cc/full/ab/wallhaven-abc1.jpg");
    assert_eq!(page.results[0].thumb_small, "https://th.wallhaven.cc/small/ab/abc1.jpg");
    assert_eq!(page.results[0].thumb_large, "https://th/large.jpg");
    assert_eq!(page.results[0].file_size, 1234567);
    assert_eq!(page.results[1].thumb_small, "https://th/large2.jpg");
    assert_eq!(page.results[1].thumb_large, "https://th/large2.jpg");
}

#[test]
fn parse_search_error() {
    assert!(parse_search(r#"{"error":"Unauthorized"}"#).is_err());
}

#[test]
fn parse_collections_basic() {
    let json = r#"{"data":[
        {"id":12,"label":"Nature","views":5,"public":1,"count":42},
        {"id":34,"label":"Anime","count":7}
    ]}"#;
    let cols = parse_collections(json).unwrap();
    assert_eq!(cols.len(), 2);
    assert_eq!(cols[0].id, 12);
    assert_eq!(cols[0].label, "Nature");
    assert_eq!(cols[0].count, 42);
    assert_eq!(cols[1].id, 34);
    assert_eq!(cols[1].count, 7);
}

#[test]
fn safe_seg_separators() {
    assert_eq!(safe_seg("../../etc/passwd"), ".._.._etc_passwd");
    assert_eq!(safe_seg("a\\b"), "a_b");
    assert_eq!(safe_seg("ab1\0x"), "ab1_x");
    assert_eq!(safe_seg("abc123"), "abc123");
}

#[test]
fn download_ssrf_refused() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    for url in [
        "http://127.0.0.1/wallhaven-abc.jpg",
        "http://169.254.169.254/latest/meta-data",
        "file:///etc/passwd",
        "https://evil.com/wallhaven-abc.jpg",
    ] {
        assert!(download(url, root, "abc").is_err(), "{url}");
    }
    let entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
    assert!(entries.is_empty());
}

#[test]
fn preview_promoted_by_link() {
    let cache = tempfile::tempdir().unwrap();
    let library = tempfile::tempdir_in(cache.path().parent().unwrap()).unwrap();
    let preview = cache.path().join("preview.png");
    image::RgbaImage::new(4, 3).save(&preview).unwrap();
    let destination = library.path().join("wallhaven-test.png");
    import_preview(&preview, &destination).unwrap();
    assert_eq!(std::fs::read(&destination).unwrap(), std::fs::read(&preview).unwrap());
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            std::fs::metadata(&destination).unwrap().ino(),
            std::fs::metadata(&preview).unwrap().ino()
        );
    }
    std::fs::remove_file(&preview).unwrap();
    assert!(destination.exists());
}

#[test]
fn malformed_preview_rejected() {
    let cache = tempfile::tempdir().unwrap();
    let library = tempfile::tempdir().unwrap();
    let preview = cache.path().join("preview.png");
    std::fs::write(&preview, b"not an image").unwrap();
    let destination = library.path().join("wallhaven-test.png");
    assert!(import_preview(&preview, &destination).is_err());
    assert!(!destination.exists());
}
