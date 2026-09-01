#![cfg(test)]

use super::*;

#[test]
fn internal_library_paths() {
    assert!(is_internal_library_path(Path::new("/walls/.skwd-wall-v2/trash/images/a.png")));
    assert!(!is_internal_library_path(Path::new("/walls/art/a.png")));
}

#[test]
fn key_for_path_dirs() {
    let walls = "/home/u/wallpaper";
    let videos = "/home/u/wallpaper-videos";
    assert_eq!(
        key_for_path(Path::new("/home/u/wallpaper/a.png"), walls, videos),
        Some("static:a.png".into())
    );
    assert_eq!(
        key_for_path(Path::new("/home/u/wallpaper-videos/sub/c.mp4"), walls, videos),
        Some("video:sub/c.mp4".into())
    );
    assert_eq!(key_for_path(Path::new("/etc/passwd"), walls, videos), None);
    assert_eq!(key_for_path(Path::new("/home/u/wallpaper"), walls, videos), None);
}

#[test]
fn key_same_dir_ext() {
    let dir = "/home/u/wallpaper";
    assert_eq!(
        key_for_path(Path::new("/home/u/wallpaper/clip.mp4"), dir, dir),
        Some("video:clip.mp4".into()),
    );
    assert_eq!(
        key_for_path(Path::new("/home/u/wallpaper/sub/a.webp"), dir, dir),
        Some("static:sub/a.webp".into())
    );
}

#[test]
fn remote_paths_confined() {
    let base = remote_thumbs_dir();
    let evil = remote_thumb("wallhaven", "../../etc/profile");
    assert!(evil.starts_with(&base));
    assert_eq!(
        evil.components().filter(|part| matches!(part, std::path::Component::ParentDir)).count(),
        0
    );
    let preview_base = cache_dir().join("remote-preview");
    let preview = remote_preview("../steam", "../../x", "../sh");
    assert!(preview.starts_with(&preview_base));
    assert_eq!(
        preview.components().filter(|part| matches!(part, std::path::Component::ParentDir)).count(),
        0
    );
}

#[test]
fn thumb_name_strips() {
    assert_eq!(thumb_name("a.png"), "a");
    assert_eq!(thumb_name("sub/b.jpg"), "sub--b");
    assert_eq!(thumb_name("noext"), "noext");
}

#[test]
fn video_thumb_prefix() {
    assert!(video_thumb_sm("clip.mp4").to_string_lossy().ends_with("/vid--clip.webp"));
}

#[test]
fn sibling_bin_fallback() {
    let exe = std::env::current_exe().unwrap();
    let name = exe.file_name().unwrap().to_str().unwrap();
    assert_eq!(sibling_bin(name), exe.parent().unwrap().join(name));
    let missing = "skwd-definitely-not-installed-xyz";
    assert_eq!(sibling_bin(missing), PathBuf::from(missing));
}

#[test]
fn binary_prefers_sibling() {
    let temp = tempfile::tempdir().unwrap();
    let sibling = temp.path().join("sibling");
    let path = temp.path().join("path");
    std::fs::create_dir_all(&sibling).unwrap();
    std::fs::create_dir_all(&path).unwrap();
    make_executable(&sibling.join("skwd-paper-v2"));
    make_executable(&path.join("skwd-paper-v2"));
    let search_path = std::env::join_paths([&path]).unwrap();

    assert_eq!(
        resolve_preferred_binary(Some(&sibling), Some(&search_path), &["skwd-paper-v2"]),
        Some(sibling.join("skwd-paper-v2"))
    );
    assert_eq!(
        resolve_preferred_binary(None, Some(&search_path), &["skwd-paper-v2"]),
        Some(path.join("skwd-paper-v2"))
    );
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

    std::fs::write(path, b"").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(not(unix))]
fn make_executable(path: &Path) {
    std::fs::write(path, b"").unwrap();
}

#[test]
fn preview_key_routes() {
    assert_eq!(preview_for_key("video:sub/clip.mp4"), Some(video_preview("sub/clip.mp4")));
    assert_eq!(preview_for_key("we:12345"), Some(we_preview("12345")));
    assert_eq!(preview_for_key("static:a.png"), None);
}

#[test]
fn safe_component_traversal() {
    for good in ["4242", "1069166728", "my-scene_2", "a.b"] {
        assert!(safe_component(good), "{good}");
    }
    for bad in ["", ".", "..", "../4242", "a/b", "a\\b", "a b", "we:4242"] {
        assert!(!safe_component(bad), "{bad}");
    }
}
