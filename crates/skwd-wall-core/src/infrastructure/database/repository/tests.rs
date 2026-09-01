#![cfg(test)]

use super::*;

fn table_names(conn: &Connection) -> Vec<String> {
    let mut stmt =
        conn.prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name").unwrap();
    let rows = stmt.query_map([], |row| row.get::<_, String>(0)).unwrap();
    rows.flatten().collect()
}

fn meta_columns(conn: &Connection) -> Vec<String> {
    let mut stmt = conn.prepare("PRAGMA table_info(meta)").unwrap();
    let rows = stmt.query_map([], |row| row.get::<_, String>(1)).unwrap();
    rows.flatten().collect()
}

#[test]
fn migrate_idempotent() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE meta(
                key TEXT PRIMARY KEY, tags TEXT, colors TEXT, matugen TEXT,
                favourite INTEGER DEFAULT 0, type TEXT, name TEXT, thumb TEXT, thumb_sm TEXT,
                video_file TEXT, we_id TEXT, mtime INTEGER, hue INTEGER DEFAULT 99,
                sat INTEGER DEFAULT 0
            );
            INSERT INTO meta(key, type, name, thumb, favourite)
                VALUES('static:old.png', 'static', 'old.png', '/t.webp', 1);",
    )
    .unwrap();

    assert!(list_wallpapers(&conn, false).is_err());

    migrate(&conn).unwrap();

    let cols = meta_columns(&conn);
    for missing in [
        "tags_raw",
        "richness",
        "analyzed_by",
        "analysis_error",
        "filesize",
        "width",
        "height",
        "duration_ms",
        "weather",
        "apply_count",
        "last_applied",
    ] {
        assert!(cols.contains(&missing.to_string()), "{missing}");
    }

    let tables = table_names(&conn);
    for table in [
        "effect_tags",
        "playlists",
        "playlist_members",
        "playlist_assign",
        "image_optimize",
        "video_convert",
        "tinier_convert",
    ] {
        assert!(tables.contains(&table.to_string()), "{table}");
    }

    let list = list_wallpapers(&conn, false).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["key"], "static:old.png");
    assert_eq!(list[0]["favourite"], 1);
    assert_eq!(list[0]["apply_count"], 0);
    assert_eq!(list[0]["last_applied"], 0);
    assert!(list[0]["weather"].is_null());

    bump_apply_count(&conn, "static:old.png").unwrap();
    assert_eq!(list_wallpapers(&conn, false).unwrap()[0]["apply_count"], 1);
    assert_eq!(list_wallpapers(&conn, false).unwrap()[0]["last_applied"], 1);

    let id = playlist_create(&conn, "Chill").unwrap();
    playlist_add_member(&conn, id, "static:old.png").unwrap();
    set_effect_tag(&conn, "old", "sepia").unwrap();

    migrate(&conn).unwrap();
    assert_eq!(playlist_members(&conn, id).unwrap(), vec!["static:old.png"]);
    assert_eq!(effect_tag(&conn, "old").as_deref(), Some("sepia"));
    assert_eq!(list_wallpapers(&conn, false).unwrap().len(), 1);
    assert_eq!(meta_columns(&conn).len(), META_COLUMNS.len() + 1, "key plus ensured columns");
}

#[test]
fn migration_purges_gen_rows() {
    let conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();
    conn.execute_batch(
        "INSERT INTO meta(key, type, name, thumb)
             VALUES('static:keep.png', 'static', 'keep.png', '/keep.webp'),
                   ('gen:old-scene', 'gen', 'Old scene', '/gen.webp'),
                   ('shader:older-scene', 'shader', 'Older scene', '/shader.webp');
         INSERT INTO analysis_tags(key, tag, score, source)
             VALUES('gen:old-scene', 'abstract', 0.9, 'test');
         INSERT INTO analysis_candidates(key, tag, score, source)
             VALUES('shader:older-scene', 'bright', 0.8, 'test');
         INSERT INTO playlists(name) VALUES('Legacy GEN playlist');
         INSERT INTO playlist_members(playlist_id, key, position)
             VALUES(last_insert_rowid(), 'gen:old-scene', 0);",
    )
    .unwrap();

    migrate(&conn).unwrap();

    assert_eq!(list_wallpapers(&conn, false).unwrap().len(), 1);
    assert_eq!(list_wallpapers(&conn, false).unwrap()[0]["key"], "static:keep.png");
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM analysis_tags", [], |row| row.get::<_, usize>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM analysis_candidates", [], |row| row
            .get::<_, usize>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM playlist_members", [], |row| row.get::<_, usize>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        conn.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0)).unwrap(),
        4
    );
}

#[test]
fn duration_upgrade_requests_one_video_rescan() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE meta(
             key TEXT PRIMARY KEY, tags TEXT, colors TEXT, matugen TEXT,
             favourite INTEGER DEFAULT 0, type TEXT, name TEXT, thumb TEXT, thumb_sm TEXT,
             video_file TEXT, we_id TEXT, mtime INTEGER, hue INTEGER DEFAULT 99,
             sat INTEGER DEFAULT 0
         );
         INSERT INTO meta(key, type, name, thumb, mtime)
             VALUES('video:a.mp4', 'video', 'a.mp4', '/t.webp', 123);
         PRAGMA user_version = 3;",
    )
    .unwrap();

    migrate(&conn).unwrap();
    assert_eq!(
        conn.query_row("SELECT mtime FROM meta WHERE key = 'video:a.mp4'", [], |row| {
            row.get::<_, i64>(0)
        }),
        Ok(0)
    );
    conn.execute("UPDATE meta SET mtime = 456 WHERE key = 'video:a.mp4'", []).unwrap();

    migrate(&conn).unwrap();
    assert_eq!(
        conn.query_row("SELECT mtime FROM meta WHERE key = 'video:a.mp4'", [], |row| {
            row.get::<_, i64>(0)
        }),
        Ok(456)
    );
}

#[test]
fn corrupt_db_quarantined() {
    let dir = std::env::temp_dir().join(format!("skwd-db-corrupt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("wall.sqlite");
    std::fs::write(&path, b"this is not a sqlite database at all............").unwrap();

    let err = super::open_at(&path).expect_err("garbage must not open");
    assert!(super::is_corruption(&err));

    let moved = super::quarantine(&path);
    assert!(!path.exists());
    assert!(std::path::Path::new(&moved).exists() && moved.contains(".corrupt-"));
    let conn = super::open_at(&path).expect("fresh db opens after quarantine");
    drop(conn);
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn database_files_private() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wall.sqlite");
    let connection = super::open_at(&path).unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    drop(connection);
}
