#![cfg(test)]

use super::*;

#[test]
fn wallpaper_round_trip() {
    let still = Wallpaper {
        kind: String::from(wall_proto::kind::STATIC),
        path: String::from("/w/a.png"),
        we_id: String::new(),
    };
    assert_eq!(wallpaper_from(&wallpaper_json(&still)), Some(still));
}

#[test]
fn empty_record_rejected() {
    assert_eq!(wallpaper_from(&json!({ "type": "static", "path": "", "we_id": "" })), None);
    assert_eq!(wallpaper_from(&json!({ "type": "we", "path": "/w/a.png", "we_id": "" })), None);
}

#[test]
fn we_scene_by_id() {
    let scene = wallpaper_from(&json!({ "type": "we", "path": "", "we_id": "2057951800" }));
    assert_eq!(scene.map(|scene| scene.we_id), Some(String::from("2057951800")));
}

#[test]
fn restored_wallpaper_only_fills_an_empty_monitor_policy() {
    let original = Wallpaper {
        kind: String::from(wall_proto::kind::VIDEO),
        path: String::from("/w/original.mp4"),
        we_id: String::new(),
    };
    let replacement = Wallpaper {
        kind: String::from(wall_proto::kind::STATIC),
        path: String::from("/w/replacement.png"),
        we_id: String::new(),
    };
    let mut pinned = json!({ "wallpaper": wallpaper_json(&replacement) });
    remember_wallpaper(&mut pinned, &original, false);
    assert_eq!(pinned.get("wallpaper").and_then(wallpaper_from), Some(replacement.clone()));

    remember_wallpaper(&mut pinned, &original, true);
    assert_eq!(pinned.get("wallpaper").and_then(wallpaper_from), Some(original.clone()));

    let mut empty = json!({});
    remember_wallpaper(&mut empty, &original, false);
    assert_eq!(empty.get("wallpaper").and_then(wallpaper_from), Some(original));
}

#[test]
fn missing_file_not_pin() {
    let still = Wallpaper {
        kind: String::from(wall_proto::kind::STATIC),
        path: String::from("/definitely/not/here.png"),
        we_id: String::new(),
    };
    assert!(!wallpaper_is_present(&still));
    let scene = Wallpaper {
        kind: String::from(wall_proto::kind::WE),
        path: String::new(),
        we_id: String::from("2057951800"),
    };
    assert!(wallpaper_is_present(&scene));
}

#[test]
fn existing_file_pin() {
    let mut path = std::env::temp_dir();
    path.push(format!("skwd-restore-pin-{}.png", std::process::id()));
    std::fs::write(&path, b"x").unwrap();
    let still = Wallpaper {
        kind: String::from(wall_proto::kind::STATIC),
        path: path.display().to_string(),
        we_id: String::new(),
    };
    assert!(wallpaper_is_present(&still));
    std::fs::remove_file(&path).ok();
    assert!(!wallpaper_is_present(&still));
}
