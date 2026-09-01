use super::*;
use crate::db;

#[test]
fn image_record_roundtrip() {
    let connection = db::open_in_memory().unwrap();
    image_optimization_record(
        &connection,
        "/walls/a.png",
        "/walls/a.webp",
        "balanced@2k",
        "webp",
        1280,
        720,
        1000,
        400,
    )
    .unwrap();
    let rows = image_optimization_records(&connection).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "/walls/a.png");
    assert_eq!(rows[0].1, "balanced@2k");
    assert_eq!(rows[0].2, 1000);
    assert!(rows[0].3 > 0);
}

#[test]
fn rename_carries_user_state() {
    let connection = db::open_in_memory().unwrap();
    connection
        .execute(
            "INSERT INTO meta(key,name,type,favourite) VALUES(?1,?2,'static',1)",
            params!["static:a.png", "a.png"],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO analysis_tags(key,tag,score,source) VALUES(?1,'forest',0.9,'model')",
            ["static:a.png"],
        )
        .unwrap();
    connection.execute("INSERT INTO playlists(name,kind) VALUES('test','curated')", []).unwrap();
    connection
        .execute(
            "INSERT INTO playlist_members(playlist_id,key,position) VALUES(1,?1,0)",
            ["static:a.png"],
        )
        .unwrap();

    rename_wallpaper_key(&connection, "static:a.png", "static:a.webp", "a.webp").unwrap();

    let row: (String, String, i64) = connection
        .query_row("SELECT key,name,favourite FROM meta", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .unwrap();
    assert_eq!(row, ("static:a.webp".into(), "a.webp".into(), 1));
    assert_eq!(
        connection
            .query_row("SELECT key FROM analysis_tags", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "static:a.webp"
    );
    assert_eq!(
        connection
            .query_row("SELECT key FROM playlist_members", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "static:a.webp"
    );
}

#[test]
fn optimize_rename_rolls_back() {
    let connection = db::open_in_memory().unwrap();
    connection
        .execute(
            "INSERT INTO meta(key,name,type) VALUES('static:a.png','a.png','static'),('static:a.webp','a.webp','static')",
            [],
        )
        .unwrap();

    let result = record_image_optimization_and_rename(
        &connection,
        "/walls/a.png",
        "/walls/a.webp",
        "v2:quality@2k",
        "webp",
        1280,
        720,
        1000,
        500,
        123,
        "static:a.png",
        "static:a.webp",
        "a.webp",
    );
    assert!(result.is_err());
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM image_optimize", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row("SELECT name FROM meta WHERE key='static:a.png'", [], |row| {
                row.get::<_, String>(0)
            })
            .unwrap(),
        "a.png"
    );
}

#[test]
fn rollback_restores_old_key() {
    let connection = db::open_in_memory().unwrap();
    connection
        .execute("INSERT INTO meta(key,name,type) VALUES('static:a.png','a.png','static')", [])
        .unwrap();
    record_image_optimization_and_rename(
        &connection,
        "/walls/a.png",
        "/walls/a.webp",
        "v2:quality@2k",
        "webp",
        1280,
        720,
        1000,
        500,
        123,
        "static:a.png",
        "static:a.webp",
        "a.webp",
    )
    .unwrap();
    let metadata: (i64, i64, i64, i64) = connection
        .query_row(
            "SELECT mtime,filesize,width,height FROM meta WHERE key='static:a.webp'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(metadata, (123, 500, 1280, 720));
    rollback_image_optimization_and_rename(
        &connection,
        "/walls/a.png",
        "static:a.png",
        "static:a.webp",
        "a.png",
    )
    .unwrap();
    assert_eq!(
        connection.query_row("SELECT key FROM meta", [], |row| row.get::<_, String>(0)).unwrap(),
        "static:a.png"
    );
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM image_optimize", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        0
    );
}
