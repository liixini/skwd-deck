#![cfg(test)]

use super::{
    PREVIEW_MAX_DECODE_ALLOC, PREVIEW_MAX_DIMENSION, PREVIEW_MAX_ENCODED_BYTES, PREVIEW_MAX_PIXELS,
    PreviewLimits, agent, copy_bounded, fetch_image, get_guarded, host_is_private, partial_file,
    require_public, require_source, require_wallhaven, scheme_host, send, stream_to_dest,
    validate_cached_preview,
};
use std::io::{Read, Write};
use std::net::TcpListener;

#[cfg(unix)]
#[test]
fn partial_files_unique_private() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("image.jpg");
    let (first, first_path, first_cleanup) = partial_file(&destination).unwrap();
    let (second, second_path, second_cleanup) = partial_file(&destination).unwrap();
    assert_ne!(first_path, second_path);
    assert_eq!(first.metadata().unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(second.metadata().unwrap().permissions().mode() & 0o777, 0o600);
    drop((first, second, first_cleanup, second_cleanup));
    assert!(!first_path.exists());
    assert!(!second_path.exists());
}

#[allow(clippy::unnecessary_wraps)]
fn allow_all(_: &str) -> anyhow::Result<()> {
    Ok(())
}

fn serve_once(declared_len: usize, body: &[u8]) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let body = body.to_vec();
    std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else { return };
        let mut buf = [0u8; 4096];
        let mut req = Vec::new();
        loop {
            match stream.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    req.extend_from_slice(&buf[..read]);
                    if req.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }
            }
        }
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {declared_len}\r\nConnection: close\r\n\r\n"
        );
        let _ = stream.write_all(&body);
        let _ = stream.flush();
    });
    format!("http://127.0.0.1:{port}/pic.png")
}

fn serve_redirect(location: &'static str) -> String {
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

fn serve_status(status: u16) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut buffer = [0_u8; 4096];
        let _ = stream.read(&mut buffer);
        let _ = write!(
            stream,
            "HTTP/1.1 {status} Failure\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        let _ = stream.flush();
    });
    format!("http://127.0.0.1:{port}/api?apikey=REAL_SECRET&q=forest")
}

fn part_files(dir: &std::path::Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".part"))
        .collect()
}

fn png_body(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(width, height, image::Rgba([1, 2, 3, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image).write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    bytes.into_inner()
}

#[test]
fn fetch_image_atomic() {
    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("pic.png");
    let body = png_body(2, 2);
    let url = serve_once(body.len(), &body);
    fetch_image(&url, &dest, allow_all).unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    assert_eq!(part_files(tmp.path()), Vec::<String>::new());
}

#[test]
fn provider_errors_redact_url() {
    let url = serve_status(401);
    let error = send(agent().get(&url), "Wallhaven").unwrap_err().to_string();
    assert_eq!(error, "Wallhaven request failed: HTTP 401");
    assert!(!error.contains("REAL_SECRET"));
    assert!(!error.contains("http://"));
}

#[test]
fn fetch_truncated_body() {
    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("pic.png");
    let body = png_body(2, 2);
    let url = serve_once(body.len() + 100, &body);
    assert!(fetch_image(&url, &dest, allow_all).is_err());
    assert!(!dest.exists());
    assert_eq!(part_files(tmp.path()), Vec::<String>::new());
}

#[test]
fn bounded_copy_over_limit() {
    let mut output = Vec::new();
    let error = copy_bounded(std::io::Cursor::new(b"123456789"), &mut output, 8).unwrap_err();
    assert!(error.to_string().contains("exceeds 8 byte limit"));
    assert!(output.len() <= 8);
}

#[test]
fn preview_limits_before_publish() {
    let body = png_body(2, 2);
    let cases = [
        PreviewLimits {
            encoded: body.len() as u64 - 1,
            dimension: PREVIEW_MAX_DIMENSION,
            pixels: PREVIEW_MAX_PIXELS,
            allocation: PREVIEW_MAX_DECODE_ALLOC,
        },
        PreviewLimits {
            encoded: PREVIEW_MAX_ENCODED_BYTES,
            dimension: 1,
            pixels: PREVIEW_MAX_PIXELS,
            allocation: PREVIEW_MAX_DECODE_ALLOC,
        },
        PreviewLimits {
            encoded: PREVIEW_MAX_ENCODED_BYTES,
            dimension: PREVIEW_MAX_DIMENSION,
            pixels: 3,
            allocation: PREVIEW_MAX_DECODE_ALLOC,
        },
        PreviewLimits {
            encoded: PREVIEW_MAX_ENCODED_BYTES,
            dimension: PREVIEW_MAX_DIMENSION,
            pixels: PREVIEW_MAX_PIXELS,
            allocation: 15,
        },
    ];

    for (index, limits) in cases.into_iter().enumerate() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join(format!("case-{index}.png"));
        assert!(
            stream_to_dest(
                std::io::Cursor::new(&body),
                &dest,
                crate::infrastructure::sniff::Kind::Image,
                limits,
            )
            .is_err()
        );
        assert!(!dest.exists());
        assert_eq!(part_files(tmp.path()), Vec::<String>::new());
    }
}

#[test]
fn rejected_preview_keeps_previous() {
    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("preview.png");
    std::fs::write(&dest, b"previous-good-cache").unwrap();
    let limits = PreviewLimits {
        encoded: PREVIEW_MAX_ENCODED_BYTES,
        dimension: PREVIEW_MAX_DIMENSION,
        pixels: PREVIEW_MAX_PIXELS,
        allocation: PREVIEW_MAX_DECODE_ALLOC,
    };
    assert!(
        stream_to_dest(
            std::io::Cursor::new(b"<html>not an image</html>"),
            &dest,
            crate::infrastructure::sniff::Kind::Image,
            limits,
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&dest).unwrap(), b"previous-good-cache");
    assert_eq!(part_files(tmp.path()), Vec::<String>::new());
}

#[test]
fn cached_preview_limits() {
    let tmp = tempfile::tempdir().unwrap();
    let good = tmp.path().join("good.png");
    std::fs::write(&good, png_body(2, 2)).unwrap();
    validate_cached_preview(&good).unwrap();

    let malformed = tmp.path().join("bad.png");
    std::fs::write(&malformed, b"\x89PNG\r\n\x1a\n\0\0\0\rIHDRxxxx").unwrap();
    assert!(validate_cached_preview(&malformed).is_err());

    let truncated = tmp.path().join("truncated.png");
    let mut bytes = png_body(8, 8);
    bytes.truncate(bytes.len() / 2);
    std::fs::write(&truncated, bytes).unwrap();
    assert!(validate_cached_preview(&truncated).is_err());
}

#[test]
fn fetch_rejects_html() {
    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("pic.jpg");
    let body: &[u8] = b"<!DOCTYPE html><html>rate limited</html>";
    let url = serve_once(body.len(), body);
    let err = fetch_image(&url, &dest, allow_all).unwrap_err();
    assert!(err.to_string().contains("HTML"));
    assert!(!dest.exists());
    assert_eq!(part_files(tmp.path()), Vec::<String>::new());
}

#[test]
fn fetch_blocks_loopback() {
    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("pic.png");
    let body = png_body(2, 2);
    let url = serve_once(body.len(), &body);
    assert!(fetch_image(&url, &dest, require_public).is_err());
    assert!(!dest.exists());
    assert_eq!(part_files(tmp.path()), Vec::<String>::new());
}

#[test]
fn steam_cdn_pins() {
    assert!(require_source("steam", "https://images.steamusercontent.com/ugc/123/x.jpg").is_ok());
    assert!(require_source("steam", "https://cdn.steamstatic.com/x.jpg").is_ok());
    assert!(require_source("steam", "https://steamuserimages-a.akamaihd.net/ugc/1/x.jpg").is_ok());
    for url in [
        "https://evil.com/x.jpg",
        "https://steamstatic.com.evil.com/x.jpg",
        "http://127.0.0.1/x.jpg",
    ] {
        assert!(require_source("steam", url).is_err(), "{url}");
    }
}

#[test]
fn public_guard_delegates() {
    assert!(require_public("http://169.254.169.254/latest/meta-data").is_err());
    assert!(require_public("https://w.wallhaven.cc/full/ab/wallhaven-abc.jpg").is_ok());
    assert!(host_is_private("::ffff:127.0.0.1"));
}

#[test]
fn wallhaven_host_pin() {
    assert!(require_wallhaven("https://w.wallhaven.cc/full/ab/wallhaven-abc.jpg").is_ok());
    assert!(require_wallhaven("https://th.wallhaven.cc/small/ab/abc.jpg").is_ok());
    assert!(require_wallhaven("https://whvn.cc/abc").is_ok());
    for url in [
        "https://evil.com/x.jpg",
        "https://wallhaven.cc.evil.com/x.jpg",
        "http://127.0.0.1/x.jpg",
        "file:///etc/passwd",
    ] {
        assert!(require_wallhaven(url).is_err(), "{url}");
    }
}

#[test]
fn source_cdn_allowlist() {
    assert!(require_source("unsplash", "https://images.unsplash.com/photo-1.jpg").is_ok());
    assert!(require_source("unsplash", "https://plus.unsplash.com/x.jpg").is_ok());
    assert!(require_source("bing", "https://www.bing.com/th?id=OHR.x_UHD.jpg").is_ok());
    assert!(require_source("wallhaven", "https://w.wallhaven.cc/full/ab/x.jpg").is_ok());
    for (src, url) in [
        ("unsplash", "https://images.unsplash.com.evil.com/x.jpg"),
        ("unsplash", "http://127.0.0.1/x.jpg"),
        ("bing", "https://evil.com/x.jpg"),
        ("bing", "http://169.254.169.254/x.jpg"),
        ("unknown", "https://images.unsplash.com/x.jpg"),
    ] {
        assert!(require_source(src, url).is_err(), "{src} {url}");
    }
}

#[test]
fn redirect_ssrf_blocked() {
    let url = serve_redirect("http://169.254.169.254/latest/meta-data");
    let policy = |target: &str| {
        let (_, host) = scheme_host(target).ok_or_else(|| anyhow::anyhow!("bad url"))?;
        if host != "127.0.0.1" && host_is_private(&host) {
            anyhow::bail!("blocked {host}");
        }
        Ok(())
    };
    let err = get_guarded(&url, policy).unwrap_err().to_string();
    assert!(err.contains("169.254.169.254"));
}

#[test]
fn redirect_followed() {
    let final_url = serve_once(3, b"ok!");
    let leaked: &'static str = Box::leak(final_url.into_boxed_str());
    let start = serve_redirect(leaked);
    let resp = get_guarded(&start, allow_all).unwrap();
    let mut body = String::new();
    resp.into_reader().read_to_string(&mut body).unwrap();
    assert_eq!(body, "ok!");
}
