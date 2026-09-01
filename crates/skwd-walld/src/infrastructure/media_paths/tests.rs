use std::path::Path;

use super::*;

#[test]
fn converted_target_pick() {
    let raw = Path::new("/w/wallhaven-abc.jpg");
    assert_eq!(
        converted_target(Some(Path::new("/w/wallhaven-abc.webp")), raw),
        Some(Path::new("/w/wallhaven-abc.webp").to_path_buf()),
    );
    assert_eq!(converted_target(Some(raw), raw), None);
    assert_eq!(converted_target(None, raw), None);
}

#[test]
fn video_route_transitions_off() {
    assert_eq!(
        video_route(false, Some("/w/a.mp4"), "/w/b.mp4", Some("/t/b.webp")),
        VideoRoute::Plain
    );
}

#[test]
fn video_route_from_source() {
    assert_eq!(
        video_route(true, Some("/w/a.mp4"), "/w/b.mp4", Some("/t/b.webp")),
        VideoRoute::Transition("/w/a.mp4")
    );
}

#[test]
fn video_route_from_thumb() {
    assert_eq!(
        video_route(true, Some("/w/a.mp4"), "/w/a.mp4", Some("/t/a.webp")),
        VideoRoute::Transition("/t/a.webp")
    );
    assert_eq!(
        video_route(true, None, "/w/a.mp4", Some("/t/a.webp")),
        VideoRoute::Transition("/t/a.webp")
    );
}

#[test]
fn video_route_no_thumb() {
    assert_eq!(video_route(true, Some("/w/a.mp4"), "/w/a.mp4", None), VideoRoute::Plain);
    assert_eq!(video_route(true, None, "/w/a.mp4", None), VideoRoute::Plain);
}

#[test]
fn await_converted_fallback() {
    let temporary = tempfile::tempdir().unwrap();
    let directory = temporary.path().to_str().unwrap().to_string();
    let raw = temporary.path().join("wallhaven-cv1.jpg");
    std::fs::write(&raw, b"jpg").unwrap();
    let converted_directory = temporary.path().to_path_buf();
    let conversion = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(120));
        std::fs::write(converted_directory.join("wallhaven-cv1.webp"), b"webp").unwrap();
        std::fs::remove_file(converted_directory.join("wallhaven-cv1.jpg")).unwrap();
    });
    let resolved = await_converted(&directory, "cv1", &raw, 3000);
    conversion.join().unwrap();
    assert!(resolved.ends_with("wallhaven-cv1.webp"));

    let raw = temporary.path().join("wallhaven-cv2.jpg");
    std::fs::write(&raw, b"jpg").unwrap();
    let resolved = await_converted(&directory, "cv2", &raw, 200);
    assert!(resolved.ends_with("wallhaven-cv2.jpg"));
}

#[test]
fn await_converted_by_bing() {
    let temporary = tempfile::tempdir().unwrap();
    let directory = temporary.path().to_str().unwrap().to_string();
    let raw = temporary.path().join("bing-daily1.jpg");
    std::fs::write(&raw, b"jpg").unwrap();
    let converted_directory = temporary.path().to_path_buf();
    let conversion = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(120));
        std::fs::write(converted_directory.join("bing-daily1.webp"), b"webp").unwrap();
        std::fs::remove_file(converted_directory.join("bing-daily1.jpg")).unwrap();
    });
    let resolved = await_converted_by(&raw, 3000, || {
        crate::infrastructure::sources::library_path(&directory, "bing", "daily1")
    });
    conversion.join().unwrap();
    assert!(resolved.ends_with("bing-daily1.webp"));
}
