#![cfg(test)]

use super::{
    fetch_bytes, key_present, parse_thumb_job, prune_targets, scan_done_payload,
    thumb_host_private, thumb_require_public, thumb_resolve_redirect, thumb_scheme_host,
};
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("skwd-scan-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn touch(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b"x").unwrap();
}

#[test]
fn key_present_static() {
    let root = TempDir::new("static");
    let wdir = root.path().join("walls");
    let vdir = root.path().join("videos");
    let we = root.path().join("we");
    touch(&wdir.join("sub/pic.png"));
    let (walls, videos) = (wdir.to_str().unwrap(), vdir.to_str().unwrap());
    assert!(key_present("static:sub/pic.png", walls, videos, &we));
    assert!(!key_present("static:gone.png", walls, videos, &we));
    assert!(key_present("static:sub", walls, videos, &we));
}

#[test]
fn key_present_video() {
    let root = TempDir::new("video");
    let wdir = root.path().join("walls");
    let vdir = root.path().join("videos");
    let we = root.path().join("we");
    touch(&vdir.join("a.mp4"));
    touch(&wdir.join("b.mp4"));
    let (walls, videos) = (wdir.to_str().unwrap(), vdir.to_str().unwrap());
    assert!(key_present("video:a.mp4", walls, videos, &we));
    assert!(key_present("video:b.mp4", walls, videos, &we));
    assert!(!key_present("video:c.mp4", walls, videos, &we));
}

#[test]
fn key_present_we() {
    let root = TempDir::new("we");
    let wdir = root.path().join("walls").to_str().unwrap().to_string();
    let vdir = root.path().join("videos").to_str().unwrap().to_string();
    let we = root.path().join("we");
    touch(&we.join("12345/project.json"));
    std::fs::create_dir_all(we.join("67890")).unwrap();
    std::fs::create_dir_all(we.join("11111/project.json")).unwrap();
    assert!(key_present("we:12345", &wdir, &vdir, &we));
    assert!(!key_present("we:67890", &wdir, &vdir, &we));
    assert!(!key_present("we:11111", &wdir, &vdir, &we));
    assert!(!key_present("we:99999", &wdir, &vdir, &we));
}

#[test]
fn key_present_unknown() {
    let root = TempDir::new("unknown");
    let wdir = root.path().join("walls").to_str().unwrap().to_string();
    let vdir = root.path().join("videos").to_str().unwrap().to_string();
    let we = root.path().join("we");
    for key in ["wallhaven:abc", "staticx.png", "", "static", "video", "we"] {
        assert!(key_present(key, &wdir, &vdir, &we), "{key:?}");
    }
}

#[test]
fn prune_targets_static() {
    let targets = prune_targets("static:sub/pic.png");
    assert_eq!(targets.len(), 4, "blocks + both thumbs");
    let names: Vec<String> = targets
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert!(names.iter().any(|name| name.ends_with(".bc7")));
    assert!(names.iter().any(|name| name.ends_with(".bc1")));
    assert!(targets[2].parent().unwrap().ends_with("thumbs"));
    assert!(targets[3].parent().unwrap().ends_with("thumbs-sm"));
    for path in &targets[..2] {
        assert!(path.parent().unwrap().ends_with("blocks"));
    }
}

#[test]
fn prune_targets_video_we() {
    let video = prune_targets("video:clip.mp4");
    assert_eq!(video.len(), 4);
    assert!(video[2].parent().unwrap().ends_with("video-thumbs"));
    for path in &video[..2] {
        let name = path.file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("vid--"));
    }
    let scene = prune_targets("we:424242");
    assert_eq!(scene.len(), 4);
    assert!(scene[2].parent().unwrap().ends_with("we-thumbs"));
    assert_eq!(scene[3].file_name().unwrap().to_string_lossy(), "we--424242.webp");
    for path in &scene[..2] {
        let name = path.file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("we--"));
    }
}

#[test]
fn prune_targets_unknown() {
    assert!(prune_targets("wallhaven:abc").is_empty());
    assert!(prune_targets("random-key").is_empty());
}

#[test]
fn scan_completion_carries_optional_correlation() {
    let paths = [PathBuf::from("/walls/a.png")];
    let correlated = scan_done_payload(1, false, Some(&paths), Some("watch-17"));
    assert_eq!(correlated["request_id"], "watch-17");
    assert_eq!(correlated["paths"], json!(["/walls/a.png"]));
    let ordinary = scan_done_payload(2, false, None, None);
    assert!(ordinary.get("request_id").is_none());
    assert!(ordinary.get("paths").is_none());
}

#[test]
fn parse_thumb_job_tab() {
    assert_eq!(parse_thumb_job("42\thttp://x/y.jpg"), Some(("42".into(), "http://x/y.jpg".into())));
    assert_eq!(parse_thumb_job("42 http://x/y.jpg"), None);
    assert_eq!(parse_thumb_job("\thttp://x"), None);
    assert_eq!(parse_thumb_job("42\t"), None);
    assert_eq!(parse_thumb_job(""), None);
    assert_eq!(parse_thumb_job("42\ta\tb"), Some(("42".into(), "a\tb".into())));
}

#[test]
fn render_template_strip() {
    let colors = json!({ "primary": "#aabbcc", "surface": "#112233" });
    let tpl = "fg={{primary}} bg={{surface.strip}} x={{primary.strip}}";
    assert_eq!(super::render_template(tpl, &colors), "fg=#aabbcc bg=112233 x=aabbcc");
}

fn serve_redirect_once(location: String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else { return };
        let mut buf = [0u8; 4096];
        let _ = stream.read(&mut buf);
        let _ = write!(
            stream,
            "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        let _ = stream.flush();
    });
    format!("http://127.0.0.1:{port}/start")
}

#[test]
fn thumb_guard_delegation() {
    assert!(thumb_require_public("http://127.0.0.1/thumb.jpg").is_err());
    assert!(thumb_require_public("https://th.wallhaven.cc/small/ab/abc.jpg").is_ok());
    assert!(thumb_host_private("::ffff:10.0.0.1"));
    assert_eq!(
        thumb_scheme_host("https://a:b@th.wallhaven.cc:443/x"),
        Some(("https".into(), "th.wallhaven.cc".into()))
    );
    assert_eq!(thumb_resolve_redirect("https://a.cc/p/q", "/z").as_deref(), Some("https://a.cc/z"));
}

#[test]
fn fetch_bytes_loopback() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let url = format!("http://127.0.0.1:{port}/thumb.jpg");
    assert!(fetch_bytes(&url).is_err());
}

#[test]
fn guarded_redirect_block() {
    use super::fetch_bytes_guarded;
    let url = serve_redirect_once("http://169.254.169.254/latest/meta-data".to_string());
    let policy = |target: &str| {
        let (_, host) =
            thumb_scheme_host(target).ok_or_else(|| anyhow::anyhow!("unparseable url"))?;
        if host != "127.0.0.1" && thumb_host_private(&host) {
            anyhow::bail!("blocked {host}");
        }
        Ok(())
    };
    let err = fetch_bytes_guarded(&url, policy).unwrap_err().to_string();
    assert!(err.contains("169.254.169.254"));
}
