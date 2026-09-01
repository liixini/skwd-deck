#![cfg(test)]

use super::*;

#[test]
fn ext_from_url_defaults() {
    assert_eq!(ext_from_url("https://i.redd.it/abc.png?width=1"), "png");
    assert_eq!(ext_from_url("https://www.bing.com/th?id=OHR.x_UHD.jpg"), "jpg");
    assert_eq!(ext_from_url("https://images.unsplash.com/photo-1"), "jpg");
}

#[test]
fn library_scoped() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("unsplash-abc.jpg"), b"x").unwrap();
    std::fs::write(dir.path().join("bing-20260709.jpg"), b"x").unwrap();
    std::fs::write(dir.path().join("wallhaven-zzz.png"), b"x").unwrap();
    let root = dir.path().to_str().unwrap();
    let unsplash = library_ids(root, "unsplash");
    assert!(unsplash.contains("abc"));
    assert_eq!(unsplash.len(), 1);
    assert!(library_path(root, "bing", "20260709").is_some());
    assert!(library_path(root, "bing", "nope").is_none());
}

#[test]
fn download_ssrf_refused() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    for url in [
        "http://127.0.0.1/x.jpg",
        "http://169.254.169.254/latest/meta-data",
        "file:///etc/passwd",
        "https://evil.com/x.jpg",
    ] {
        let mut ticks = 0u32;
        assert!(
            download_with_progress("unsplash", url, root, "abc", &mut |_| ticks += 1).is_err(),
            "{url}"
        );
        assert_eq!(ticks, 0);
    }
    let entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
    assert!(entries.is_empty());
}
