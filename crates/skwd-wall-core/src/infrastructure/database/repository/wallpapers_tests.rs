#![cfg(test)]

use super::*;
use crate::db::{open_in_memory, seed, set_effect_tag};

#[test]
fn retire_video_converts_clears() {
    let connection = open_in_memory().unwrap();
    connection
        .execute(
            "INSERT INTO video_convert(src, dest, preset, codec, orig_size, new_size, converted_at)
             VALUES('/videos/a.mp4', '/cache/a.av1.mp4', 'old', 'av1', 1000, 400, 0),
                   ('/videos/b.mp4', '/videos/b.mp4', 'skip', 'h264', 500, 500, 0)",
            [],
        )
        .unwrap();

    assert_eq!(retire_video_converts(&connection).unwrap(), ["/cache/a.av1.mp4"]);
    assert_eq!(
        connection.query_row("SELECT count(*) FROM video_convert", [], |row| row.get::<_, i64>(0)),
        Ok(0)
    );
}

#[test]
fn tinier_convert_roundtrip() {
    let connection = open_in_memory().unwrap();
    tinier_convert_record(
        &connection,
        "/vids/v.mp4",
        "/cache/v.tinier-v1.ivf",
        "30000/1001",
        "tinier-av1-v1",
        10,
        300,
    )
    .unwrap();
    assert_eq!(
        tinier_convert_entry(&connection, "/vids/v.mp4").unwrap(),
        Some(("/cache/v.tinier-v1.ivf".into(), "30000/1001".into(), "tinier-av1-v1".into(), 10,))
    );
    assert_eq!(
        tinier_convert_src(&connection, "/cache/v.tinier-v1.ivf").unwrap(),
        Some("/vids/v.mp4".into())
    );
    assert_eq!(
        tinier_convert_delete(&connection, "/vids/v.mp4").unwrap(),
        Some("/cache/v.tinier-v1.ivf".into())
    );
    assert_eq!(tinier_convert_entry(&connection, "/vids/v.mp4").unwrap(), None);
}

#[test]
fn upsert_list_roundtrip() {
    let conn = open_in_memory().unwrap();
    seed(&conn, "static:a.png", "a.png", "static");
    seed(&conn, "static:b.png", "b.png", "static");
    let list = list_wallpapers(&conn, false).unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["name"], "a.png");
    assert_eq!(list[0]["width"], 1920);
    assert_eq!(list[0]["type"], "static");
    assert!(list[0]["duration_ms"].is_null());
}

#[test]
fn indexed_duration_roundtrip() {
    let conn = open_in_memory().unwrap();
    seed(&conn, "video:a.mp4", "a.mp4", "video");
    assert_eq!(update_duration(&conn, "video:a.mp4", 92_500).unwrap(), 1);
    let list = list_wallpapers(&conn, false).unwrap();
    assert_eq!(list[0]["duration_ms"], 92_500);
    let (json, _) = list_wallpapers_json(&conn, false).unwrap();
    let typed: Vec<wall_proto::WallpaperItem> = serde_json::from_str(&json).unwrap();
    assert_eq!(typed[0].duration_ms, Some(92_500));
}

#[test]
fn list_json_matches_tree() {
    let conn = open_in_memory().unwrap();
    seed(&conn, "static:a.png", "a.png", "static");
    seed(&conn, "video:clips/b.mp4", "b.mp4", "video");
    seed(&conn, "static:effects/c.png", "c.png", "static");
    set_effect_tag(&conn, "effects/c", "negative").unwrap();
    set_favourite(&conn, "static:a.png", true).unwrap();

    for fav in [false, true] {
        let tree = serde_json::Value::Array(list_wallpapers(&conn, fav).unwrap());
        let (json, count) = list_wallpapers_json(&conn, fav).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(parsed, tree);
        assert_eq!(count, tree.as_array().unwrap().len());
        let typed: Vec<wall_proto::WallpaperItem> =
            serde_json::from_str(&json).expect("shared wire type");
        assert_eq!(typed.len(), count);
        assert!(typed.iter().all(|item| item.key.is_some() && item.kind.is_some()));
    }
}

#[test]
fn thumb_after_user_tags() {
    let conn = open_in_memory().unwrap();
    seed(&conn, "static:a.png", "a.png", "static");
    assert_eq!(thumb_for_key(&conn, "static:a.png").unwrap().as_deref(), Some("/t.webp"));
    assert!(update_user_tags(&conn, "static:a.png", "forest,my-trip").unwrap());
    assert_eq!(thumb_for_key(&conn, "static:a.png").unwrap().as_deref(), Some("/t.webp"));
    assert!(thumb_for_key(&conn, "static:missing").unwrap().is_none());
}

#[test]
fn known_we_meta_rows() {
    let conn = open_in_memory().unwrap();
    upsert_cache_entry(
        &conn, "we:111", "we", "scene", "/t.webp", "/s.webp", "", "111", 10, 4, 50, 200, 0, 1920,
        1080,
    )
    .unwrap();
    upsert_cache_entry(
        &conn,
        "we:222",
        "video",
        "clip",
        "/t2.webp",
        "/s2.webp",
        "/we/222/clip.mp4",
        "",
        20,
        4,
        50,
        200,
        999,
        1920,
        1080,
    )
    .unwrap();
    upsert_cache_entry(
        &conn,
        "static:a.png",
        "static",
        "a.png",
        "/t3.webp",
        "/s3.webp",
        "",
        "",
        30,
        4,
        50,
        200,
        0,
        1920,
        1080,
    )
    .unwrap();
    let mut got = known_we_meta(&conn).unwrap();
    got.sort();
    assert_eq!(got.len(), 2);
    assert_eq!(got[0], ("we:111".to_string(), 10, "we".to_string(), String::new()));
    assert_eq!(
        got[1],
        ("we:222".to_string(), 20, "video".to_string(), "/we/222/clip.mp4".to_string())
    );
}

#[test]
fn thumb_by_path() {
    let conn = open_in_memory().unwrap();
    upsert_cache_entry(
        &conn,
        "we:222",
        "video",
        "clip",
        "/cache/222.webp",
        "/cache/222.sm.webp",
        "/we/222/clip.mp4",
        "",
        20,
        4,
        50,
        200,
        999,
        1920,
        1080,
    )
    .unwrap();
    assert_eq!(
        thumb_for_video(&conn, "/we/222/clip.mp4").unwrap().as_deref(),
        Some("/cache/222.webp")
    );
    assert_eq!(thumb_for_video(&conn, "/nope.mp4").unwrap(), None);
}

#[test]
fn favourite_toggle() {
    let conn = open_in_memory().unwrap();
    seed(&conn, "static:a.png", "a.png", "static");
    assert!(set_favourite(&conn, "static:a.png", true).unwrap());
    assert_eq!(list_wallpapers(&conn, true).unwrap().len(), 1);
    assert!(set_favourite(&conn, "static:a.png", false).unwrap());
    assert_eq!(list_wallpapers(&conn, true).unwrap().len(), 0);
    assert!(!set_favourite(&conn, "missing", true).unwrap());
}

#[test]
fn update_user_tags_roundtrip() {
    let conn = open_in_memory().unwrap();
    seed(&conn, "static:a.png", "a.png", "static");
    assert!(update_user_tags(&conn, "static:a.png", "forest,green").unwrap());
    let list = list_wallpapers(&conn, false).unwrap();
    assert_eq!(list[0]["tags"], "forest,green");
}

#[test]
fn bump_delete_keys() {
    let conn = open_in_memory().unwrap();
    seed(&conn, "static:a.png", "a.png", "static");
    seed(&conn, "static:b.png", "b.png", "static");
    bump_apply_count(&conn, "static:a.png").unwrap();
    bump_apply_count(&conn, "static:b.png").unwrap();
    bump_apply_count(&conn, "static:a.png").unwrap();
    let list = list_wallpapers(&conn, false).unwrap();
    let first = list.iter().find(|item| item["key"] == "static:a.png").unwrap();
    let second = list.iter().find(|item| item["key"] == "static:b.png").unwrap();
    assert_eq!(first["apply_count"], 2);
    assert_eq!(first["last_applied"], 3);
    assert_eq!(second["apply_count"], 1);
    assert_eq!(second["last_applied"], 2);
    assert_eq!(key_for_video_file(&conn, "/not/a/video").unwrap(), None);
    assert!(has_entry(&conn, "static:a.png"));
    assert_eq!(known_keys(&conn).unwrap().len(), 2);
    assert!(delete_by_name(&conn, "a.png").unwrap());
    assert!(!has_entry(&conn, "static:a.png"));
}

#[test]
fn clear_cache_empties() {
    let conn = open_in_memory().unwrap();
    seed(&conn, "static:a.png", "a.png", "static");
    seed(&conn, "static:b.png", "b.png", "static");
    assert_eq!(clear_cache(&conn).unwrap(), 2);
    assert_eq!(list_wallpapers(&conn, false).unwrap().len(), 0);
}

#[test]
fn random_pick_filters() {
    let conn = open_in_memory().unwrap();
    seed(&conn, "static:a.png", "a.png", "static");
    upsert_cache_entry(
        &conn,
        "video:v.mp4",
        "video",
        "v.mp4",
        "/t.webp",
        "/s.webp",
        "/abs/v.mp4",
        "",
        100,
        4,
        50,
        200,
        1,
        1920,
        1080,
    )
    .unwrap();
    set_favourite(&conn, "video:v.mp4", true).unwrap();

    let only_static = random_pick(&conn, None, &["static"], false).unwrap().unwrap();
    assert_eq!(only_static.1, "static");
    assert_eq!(only_static.0, "static:a.png");

    let fav = random_pick(&conn, None, &["static", "video"], true).unwrap().unwrap();
    assert_eq!(fav.0, "video:v.mp4");
    assert_eq!(fav.3, "/abs/v.mp4");

    let excluded = random_pick(&conn, Some("a.png"), &["static"], false).unwrap();
    assert!(excluded.is_none());

    assert!(random_pick(&conn, None, &[], false).unwrap().is_none());
}

#[test]
fn color_rows_update() {
    let conn = open_in_memory().unwrap();
    seed(&conn, "static:a.png", "a.png", "static");
    let rows = color_rows(&conn).unwrap();
    assert_eq!(rows, vec![("static:a.png".to_string(), "/t.webp".to_string())]);
    assert_eq!(update_colors(&conn, "static:a.png", 7, 80, 300).unwrap(), 1);
    let list = list_wallpapers(&conn, false).unwrap();
    assert_eq!(list[0]["hue"], 7);
    assert_eq!(list[0]["sat"], 80);
    assert_eq!(list[0]["richness"], 300);
}

#[test]
fn captured_thumbnail_updates_legacy_paths_without_changing_user_metadata() {
    let conn = open_in_memory().unwrap();
    upsert_cache_entry(
        &conn,
        "we:42",
        "we",
        "scene",
        "/old/we-thumbs/42.webp",
        "/old/thumbs-sm/we--42.webp",
        "",
        "42",
        123,
        5,
        60,
        200,
        0,
        160,
        160,
    )
    .unwrap();
    set_favourite(&conn, "we:42", true).unwrap();
    update_user_tags(&conn, "we:42", "user tag").unwrap();
    bump_apply_count(&conn, "we:42").unwrap();
    assert_eq!(
        update_thumbnail_paths(
            &conn,
            "we:42",
            "/new/we-thumbs/42.webp",
            "/new/thumbs-sm/we--42.webp",
            Some(&serde_json::json!({"input": "scene", "artifacts": []})),
            true,
        )
        .unwrap(),
        1
    );
    let row = conn.query_row("SELECT thumb, thumb_sm, favourite, tags, apply_count, mtime FROM meta WHERE key = 'we:42'", [], |row| Ok((row.get::<_, String>(0)?,row.get::<_, String>(1)?,row.get::<_, i64>(2)?,row.get::<_, String>(3)?,row.get::<_, i64>(4)?,row.get::<_, i64>(5)?))).unwrap();
    assert_eq!(
        row,
        (
            "/new/we-thumbs/42.webp".into(),
            "/new/thumbs-sm/we--42.webp".into(),
            1,
            "user tag".into(),
            1,
            123
        )
    );
    assert_eq!(
        update_thumbnail_paths(&conn, "we:missing", "/new/missing", "/new/small", None, true)
            .unwrap(),
        0
    );
}

#[test]
fn thumbnail_record_survives_reopen_and_is_removed_with_the_wallpaper() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("wall.sqlite");
    let record = serde_json::json!({"input": {"version": 1}, "artifacts": []});
    {
        let conn = super::super::connection::open_at(&path).unwrap();
        seed(&conn, "we:42", "scene", "we");
        assert!(thumbnail_cache(&conn, "we:42").unwrap().is_none());
        update_thumbnail_paths(&conn, "we:42", "/full.webp", "/small.webp", Some(&record), true)
            .unwrap();
    }
    let conn = super::super::connection::open_at(&path).unwrap();
    assert_eq!(thumbnail_cache(&conn, "we:42").unwrap(), Some(record));
    assert_eq!(list_wallpapers(&conn, false).unwrap()[0]["thumbnail_generated"], true);
    clear_cache(&conn).unwrap();
    assert!(thumbnail_cache(&conn, "we:42").unwrap().is_none());
}

#[test]
fn thumbnail_reset_invalidates_only_the_capture_record() {
    let conn = open_in_memory().unwrap();
    seed(&conn, "we:42", "scene", "we");
    assert!(invalidate_thumbnail_cache(&conn, "we:42").unwrap());
    seed(&conn, "static:still.png", "still.png", "static");
    assert!(!invalidate_thumbnail_cache(&conn, "static:still.png").unwrap());
    let record = serde_json::json!({"input": "scene", "artifacts": []});
    update_thumbnail_paths(&conn, "we:42", "/generated.webp", "/small.webp", Some(&record), true)
        .unwrap();
    let before = list_wallpapers(&conn, false).unwrap();
    assert!(invalidate_thumbnail_cache(&conn, "we:42").unwrap());
    assert!(thumbnail_cache(&conn, "we:42").unwrap().is_none());
    assert_eq!(list_wallpapers(&conn, false).unwrap(), before);
    assert!(!invalidate_thumbnail_cache(&conn, "we:missing").unwrap());
    update_thumbnail_paths(&conn, "we:42", "/fresh.webp", "/small.webp", Some(&record), true)
        .unwrap();
    assert_eq!(thumbnail_cache(&conn, "we:42").unwrap(), Some(record));
}

#[test]
fn invalid_thumbnail_record_is_a_cache_miss() {
    let conn = open_in_memory().unwrap();
    seed(&conn, "we:42", "scene", "we");
    conn.execute("UPDATE meta SET thumbnail_cache = 'incomplete' WHERE key = 'we:42'", []).unwrap();
    assert!(thumbnail_cache(&conn, "we:42").unwrap().is_none());
}
