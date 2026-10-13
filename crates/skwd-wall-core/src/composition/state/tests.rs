use super::WallState;

#[test]
fn state_composes_independently_owned_core_services() {
    let state = WallState::test_new(serde_json::json!({}));

    let config = state.config_store();
    let database = state.database();
    let renderers = state.renderer_supervisor();
    let apply = state.apply_runtime();

    assert_eq!(config.read().display().fill_mode(), "fill");
    assert_eq!(database.with_connection(crate::db::item_count).unwrap(), 0);
    assert_eq!(renderers.wallpaper_count(), 0);
    assert_eq!(apply.generation(), 0);
}

#[test]
fn video_identity_owner() {
    use crate::db;
    use serde_json::json;
    use std::path::Path;

    for owner in ["/walls/same.mp4", "/videos/same.mp4"] {
        for folders in [json!(["/walls", "/videos"]), json!(["/videos", "/walls"]), json!([])] {
            let state = WallState::test_new(
                json!({"paths":{"wallpaper":"/walls","videoWallpaper":"/videos"},"library":{"folders":folders}}),
            );
            state
                .with_db(|connection| {
                    db::seed(connection, "video:same.mp4", "same.mp4", "video");
                    connection.execute(
                        "UPDATE meta SET video_file=?1, favourite=1, apply_count=7",
                        [owner],
                    )?;
                    Ok(())
                })
                .unwrap();
            if folders.as_array().unwrap().is_empty() {
                assert!(state.library_key(Path::new(owner)).is_none());
                continue;
            }
            assert_eq!(state.library_key(Path::new(owner)).unwrap(), "video:same.mp4");
            let other =
                if owner.starts_with("/walls/") { "/videos/same.mp4" } else { "/walls/same.mp4" };
            assert_eq!(state.library_key(Path::new(other)).unwrap(), format!("video:{other}"));
            state.reconcile_library().unwrap();
            let stored = state
                .with_db(|connection| db::video_file_for_key(connection, "video:same.mp4"))
                .unwrap();
            assert_eq!(stored.as_deref(), Some(owner));
            assert_eq!(
                state.with_db(|connection| db::list_wallpapers(connection, true)).unwrap().len(),
                1
            );
        }
    }
}

#[test]
fn legacy_relative_anchors() {
    use crate::db;
    use serde_json::json;
    use std::path::Path;

    let state = WallState::test_new(
        json!({"paths":{"wallpaper":"unused/../walls","videoWallpaper":"videos"}}),
    );
    state
        .with_db(|connection| {
            db::seed(connection, "static:a.png", "a.png", "static");
            db::seed(connection, "video:same.mp4", "same.mp4", "video");
            connection.execute(
                "UPDATE meta SET video_file='unused/../walls/same.mp4' WHERE type='video'",
                [],
            )?;
            db::set_favourite(connection, "static:a.png", true)?;
            Ok(())
        })
        .unwrap();
    let directory = std::env::current_dir().unwrap();
    assert_eq!(state.library_key(&directory.join("walls/a.png")).unwrap(), "static:a.png");
    assert_eq!(state.library_key(&directory.join("walls/same.mp4")).unwrap(), "video:same.mp4");
    for (folders, count) in [(json!([]), 0), (json!([directory.join("walls")]), 2)] {
        let policy = skwd_config::LibraryPolicy::new(&json!({"library":{"folders":folders}}));
        state
            .with_db(|connection| {
                db::reconcile_library(connection, Path::new("unused/../walls"), &policy)
            })
            .unwrap();
        assert_eq!(state.with_db(db::item_count).unwrap(), count);
    }
    let retained = state.with_db(|connection| db::list_wallpapers(connection, true)).unwrap();
    assert_eq!(retained.len(), 1);
    assert_eq!(retained[0]["key"], "static:a.png");
    let (encoded, _) = state
        .with_db(|connection| {
            db::list_wallpapers_json(
                connection,
                false,
                "unused/../walls",
                std::path::Path::new("/we"),
                |_, _| false,
            )
        })
        .unwrap();
    let rows: Vec<serde_json::Value> = serde_json::from_str(&encoded).unwrap();
    assert!(
        rows.iter().any(|row| row["path"] == directory.join("walls/a.png").display().to_string())
    );
}
