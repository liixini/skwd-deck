#![cfg(feature = "media")]

use std::path::{Path, PathBuf};

use skwd_wall_core::{WallState, db, scan};

fn write_bmp(path: &Path, w: u32, h: u32, bgr: [u8; 3]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let row = (w * 3).div_ceil(4) * 4;
    let pixel_bytes = row * h;
    let file_size = 54 + pixel_bytes;
    let mut buf = Vec::with_capacity(file_size as usize);
    buf.extend_from_slice(b"BM");
    buf.extend_from_slice(&file_size.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&54u32.to_le_bytes());
    buf.extend_from_slice(&40u32.to_le_bytes());
    buf.extend_from_slice(&(w as i32).to_le_bytes());
    buf.extend_from_slice(&(h as i32).to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes());
    buf.extend_from_slice(&24u16.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&pixel_bytes.to_le_bytes());
    buf.extend_from_slice(&2835i32.to_le_bytes());
    buf.extend_from_slice(&2835i32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    for _ in 0..h {
        for _ in 0..w {
            buf.extend_from_slice(&bgr);
        }
        buf.extend(std::iter::repeat_n(0u8, (row - w * 3) as usize));
    }
    std::fs::write(path, buf).unwrap();
}

fn bump_mtime(path: &Path) {
    let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(10);
    file.set_modified(later).unwrap();
}

fn scan_keys(state: &WallState) -> Vec<String> {
    let items = std::sync::Mutex::new(Vec::new());
    scan::scan(state, |row| {
        items.lock().unwrap().push(row["key"].as_str().unwrap_or("").to_string());
    });
    let mut keys = items.into_inner().unwrap();
    keys.sort();
    keys
}

#[test]
fn scan_regen_rules() {
    let root = tempfile::tempdir().unwrap();
    let wall = root.path().join("wallpapers");
    let cfg_dir = root.path().join("config");
    std::fs::create_dir_all(&cfg_dir).unwrap();
    unsafe {
        std::env::set_var("XDG_DATA_HOME", root.path().join("data"));
        std::env::set_var("XDG_CACHE_HOME", root.path().join("cache"));
        std::env::set_var("SKWD_WALL_V2_CONFIG", &cfg_dir);
        std::env::set_var("SKWD_SCAN_THREADS", "1");
    }
    std::fs::write(
        cfg_dir.join("config.json"),
        format!(r#"{{"paths": {{"wallpaper": "{}"}}}}"#, wall.display()),
    )
    .unwrap();

    let a = wall.join("a.bmp");
    let b = wall.join("sub").join("b.bmp");
    write_bmp(&a, 8, 8, [40, 40, 200]);
    write_bmp(&b, 8, 8, [200, 40, 40]);

    let state = WallState::open().unwrap();

    let first = scan_keys(&state);
    assert_eq!(first, vec!["static:a.bmp", "static:sub/b.bmp"]);

    let thumbs = root.path().join("cache/skwd-wall-v2/thumbs");
    let thumb_a = thumbs.join("a.webp");
    let thumb_b = thumbs.join("sub--b.webp");
    assert!(thumb_a.is_file());
    assert!(thumb_b.is_file());
    assert!(root.path().join("cache/skwd-wall-v2/thumbs-sm/a.webp").is_file());
    assert_eq!(state.with_db(db::item_count).unwrap(), 2);

    assert!(scan_keys(&state).is_empty());

    bump_mtime(&a);
    assert_eq!(scan_keys(&state), vec!["static:a.bmp"]);

    std::fs::remove_file(&thumb_b).unwrap();
    assert_eq!(scan_keys(&state), vec!["static:sub/b.bmp"],);

    let changed: Vec<PathBuf> = vec![a, root.path().join("elsewhere.png")];
    let count = scan::scan_paths(&state, &changed, |_| {});
    assert_eq!(count, 0);

    bump_mtime(&b);
    let hits = std::sync::Mutex::new(Vec::new());
    let count = scan::scan_paths(&state, std::slice::from_ref(&b), |row| {
        hits.lock().unwrap().push(row["key"].as_str().unwrap_or("").to_string());
    });
    assert_eq!(count, 1);
    assert_eq!(hits.into_inner().unwrap(), vec!["static:sub/b.bmp"]);
    assert_eq!(state.with_db(db::item_count).unwrap(), 2);
}
