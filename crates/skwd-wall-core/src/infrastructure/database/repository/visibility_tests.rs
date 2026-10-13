use super::*;
use crate::db;
use serde_json::json;

#[test]
fn exclusion_preserves_metadata() {
    let connection = db::open_in_memory().unwrap();
    db::seed(&connection, "static:Sus/a.png", "Sus/a.png", "static");
    db::seed(&connection, "static:Sushi/b.png", "Sushi/b.png", "static");
    db::set_favourite(&connection, "static:Sus/a.png", true).unwrap();
    db::update_user_tags(&connection, "static:Sus/a.png", "kept").unwrap();
    db::bump_apply_count(&connection, "static:Sus/a.png").unwrap();
    let playlist = db::playlist_create(&connection, "Saved").unwrap();
    db::playlist_add_member(&connection, playlist, "static:Sus/a.png").unwrap();
    let before = db::list_wallpapers(&connection, true).unwrap();
    let policy = skwd_config::LibraryPolicy::new(
        &json!({"paths":{"wallpaper":"/walls"},"library":{"excludedFolders":["Sus"]}}),
    );
    assert_eq!(reconcile_library(&connection, Path::new("/walls"), &policy).unwrap(), 1);
    assert_eq!(db::item_count(&connection).unwrap(), 1);
    assert!(db::list_wallpapers(&connection, true).unwrap().is_empty());
    assert!(db::playlist_member_items(&connection, playlist).unwrap().is_empty());
    assert_eq!(db::playlist_members(&connection, playlist).unwrap(), ["static:Sus/a.png"]);
    assert!(db::random_pick(&connection, None, &["static"], true, None).unwrap().is_none());
    let (_, count) = db::list_wallpapers_json(
        &connection,
        false,
        "/walls/",
        std::path::Path::new("/we"),
        |_, _| false,
    )
    .unwrap();
    assert_eq!(count, 1);
    let policy = skwd_config::LibraryPolicy::new(
        &json!({"paths":{"wallpaper":"/walls","videoWallpaper":"/videos"}}),
    );
    assert_eq!(reconcile_library(&connection, Path::new("/walls"), &policy).unwrap(), 1);
    assert_eq!(db::list_wallpapers(&connection, true).unwrap(), before);
    assert_eq!(db::playlist_member_items(&connection, playlist).unwrap().len(), 1);
}

#[test]
fn video_source_controls_visibility() {
    let connection = db::open_in_memory().unwrap();
    db::seed(&connection, "video:a.mp4", "a.mp4", "video");
    connection.execute("UPDATE meta SET video_file = '/walls/Sus/a.mp4'", []).unwrap();
    let policy = skwd_config::LibraryPolicy::new(
        &json!({"paths":{"wallpaper":"/walls","videoWallpaper":"/videos"},"library":{"excludedFolders":["/walls/Sus"]}}),
    );
    reconcile_library(&connection, Path::new("/walls"), &policy).unwrap();
    assert_eq!(db::item_count(&connection).unwrap(), 0);
    connection.execute("UPDATE meta SET video_file = '/videos/a.mp4'", []).unwrap();
    reconcile_library(&connection, Path::new("/walls"), &policy).unwrap();
    assert_eq!(db::item_count(&connection).unwrap(), 1);
}

#[test]
fn stale_writers_remain_hidden() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("library.sqlite");
    let reader = super::super::connection::open_at(&database).unwrap();
    let writer = super::super::connection::open_at(&database).unwrap();
    let policy = skwd_config::LibraryPolicy::new(
        &json!({"paths":{"wallpaper":"/walls","videoWallpaper":"/videos"},"library":{"excludedFolders":["/walls/Sus"]}}),
    );
    reconcile_library(&reader, Path::new("/walls"), &policy).unwrap();
    db::seed(&writer, "static:Sus/late.png", "Sus/late.png", "static");
    db::set_favourite(&writer, "static:Sus/late.png", true).unwrap();
    let playlist = db::playlist_create(&reader, "Saved").unwrap();
    db::playlist_add_member(&reader, playlist, "static:Sus/late.png").unwrap();
    assert!(db::list_wallpapers(&reader, false).unwrap().is_empty());
    assert_eq!(
        db::list_wallpapers_json(&reader, false, "/walls/", std::path::Path::new("/we"), |_, _| {
            false
        })
        .unwrap()
        .1,
        0
    );
    assert_eq!(db::item_count(&reader).unwrap(), 0);
    assert!(db::random_pick(&reader, None, &["static"], false, None).unwrap().is_none());
    assert!(db::playlist_member_items(&reader, playlist).unwrap().is_empty());
    assert!(!db::library_visible(&reader, "static:Sus/late.png").unwrap());
    let policy = skwd_config::LibraryPolicy::new(
        &json!({"paths":{"wallpaper":"/walls","videoWallpaper":"/videos"}}),
    );
    reconcile_library(&reader, Path::new("/walls"), &policy).unwrap();
    assert_eq!(db::list_wallpapers(&reader, true).unwrap().len(), 1);
    assert_eq!(db::playlist_members(&reader, playlist).unwrap(), ["static:Sus/late.png"]);
}

#[test]
fn sources_preserve_metadata() {
    let connection = db::open_in_memory().unwrap();
    db::seed(&connection, "static:old.png", "old.png", "static");
    db::seed(&connection, "static:/new/a.png", "/new/a.png", "static");
    db::seed(&connection, "we:123", "scene", "we");
    db::seed(&connection, "we:456", "movie", "video");
    connection
        .execute("UPDATE meta SET video_file = '/steam/movie.mp4' WHERE key = 'we:456'", [])
        .unwrap();
    assert!(db::key_for_video_file(&connection, "/steam/movie.mp4").unwrap().is_none());
    db::set_favourite(&connection, "static:/new/a.png", true).unwrap();
    db::bump_apply_count(&connection, "static:/new/a.png").unwrap();
    let saved = db::list_wallpapers(&connection, true).unwrap();
    for (folders, count) in [(json!(["/walls"]), 3), (json!([]), 2), (json!(["/new"]), 3)] {
        let policy = skwd_config::LibraryPolicy::new(&json!({"library":{"folders":folders}}));
        reconcile_library(&connection, Path::new("/walls"), &policy).unwrap();
        assert_eq!(db::item_count(&connection).unwrap(), count);
    }
    assert_eq!(db::list_wallpapers(&connection, true).unwrap(), saved);
    let (encoded, _) = db::list_wallpapers_json(
        &connection,
        false,
        "/walls",
        std::path::Path::new("/we"),
        |_, _| false,
    )
    .unwrap();
    let rows: Vec<serde_json::Value> = serde_json::from_str(&encoded).unwrap();
    assert!(rows.iter().any(|row| row["path"] == "/new/a.png"));
}
