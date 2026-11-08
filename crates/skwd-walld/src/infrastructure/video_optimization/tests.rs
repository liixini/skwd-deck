use super::*;
use crate::testenv;
use serde_json::json;

const LEGACY_TABLE: &str = "CREATE TABLE tinier_convert(
    src TEXT PRIMARY KEY,
    dest TEXT NOT NULL,
    frame_rate TEXT NOT NULL,
    preset TEXT NOT NULL,
    orig_size INTEGER,
    new_size INTEGER,
    converted_at INTEGER
);";

fn legacy_rows(state: &skwd_wall_core::WallState, rows: &[(&Path, &Path)]) {
    state
        .with_db(|connection| {
            skwd_wall_core::db::drop_tinier_converts(connection)?;
            connection.execute_batch(LEGACY_TABLE)?;
            for (source, dest) in rows {
                connection.execute(
                    "INSERT INTO tinier_convert(src, dest, frame_rate, preset) VALUES(?1, ?2, '30/1', 'tinier-av1-v1')",
                    [source.to_str().unwrap(), dest.to_str().unwrap()],
                )?;
            }
            Ok(())
        })
        .unwrap();
}

fn table_gone(state: &skwd_wall_core::WallState) -> bool {
    state.with_db(skwd_wall_core::db::retiring_tinier_converts).unwrap().is_none()
}

fn artifact(root: &Path, relative: &str) -> PathBuf {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"av1").unwrap();
    path
}

#[test]
fn general_variant_match() {
    let root = Path::new("/cache/video-opt");
    for name in ["a.av1.mp4", "a.vp9.mp4", "a.h264-lean.mp4"] {
        assert!(is_general_variant(root, &root.join(name)));
    }
    assert!(!is_general_variant(root, &root.join("a.ivf")));
    assert!(!is_general_variant(root, Path::new("/videos/a.av1.mp4")));
    assert!(!is_general_variant(root, &root.join("nested/a.av1.mp4")));
}

#[test]
fn repoints_saved_paths() {
    let (_guard, root) = testenv::lock();
    testenv::write_config(json!({}));
    let (state, _, _) = testenv::harness();
    let source = root.join("videos/clip.mp4");
    let dest = artifact(&root, "legacy/video-opt/clip.tinier-v1.ivf");
    legacy_rows(&state, &[(&source, &dest)]);
    let entry = json!({"type": "video", "path": dest, "we_id": ""});
    let cache = state.config().cache_dir();
    let files = tinier_state_files(&state);
    skwd_wall_core::audio::write_state(&cache, &json!({"DP-1": entry}));
    let history = json!({"DP-1": {"entries": [entry], "cursor": 0}});
    std::fs::write(&files[1], history.to_string()).unwrap();
    std::fs::write(&files[2], json!({"DP-1": entry}).to_string()).unwrap();
    std::fs::write(&files[3], entry.to_string()).unwrap();
    std::fs::write(&files[4], json!({"any": entry, "portrait": entry}).to_string()).unwrap();
    std::fs::write(&files[5], json!({"id": {"wallpaper": entry}}).to_string()).unwrap();

    retire_tinier(&state);

    for file in &files {
        let text = std::fs::read_to_string(file).unwrap();
        assert!(text.contains(source.to_str().unwrap()), "{}", file.display());
        assert!(!text.contains(dest.to_str().unwrap()), "{}", file.display());
    }
    assert!(!dest.exists());
    assert!(table_gone(&state));
    let saved = std::fs::read(&files[3]).unwrap();
    retire_tinier(&state);
    assert_eq!(std::fs::read(&files[3]).unwrap(), saved);
}

#[test]
fn keeps_foreign_files() {
    let (_guard, root) = testenv::lock();
    testenv::write_config(json!({}));
    let (state, _, _) = testenv::harness();
    let cached = artifact(&root, "legacy/video-opt/cached.ivf");
    let foreign = artifact(&root, "videos/foreign.ivf");
    let same = artifact(&root, "legacy/video-opt/same.mp4");
    let orphan = artifact(&root, "legacy/video-opt/orphan.tinier-v1.ivf");
    let report = artifact(&root, "legacy/video-opt/skwd-wall-report.txt");
    let elsewhere = artifact(&root, "videos/stray.tinier-v1.ivf");
    let missing = root.join("legacy/video-opt/missing.ivf");
    let source = root.join("videos/source.mp4");
    let gone_source = root.join("videos/gone.mp4");
    let foreign_source = root.join("videos/foreign.mp4");
    legacy_rows(
        &state,
        &[
            (&source, &cached),
            (&foreign_source, &foreign),
            (&same, &same),
            (&gone_source, &missing),
        ],
    );

    retire_tinier(&state);

    assert!(!cached.exists());
    assert!(!orphan.exists());
    assert!(foreign.exists());
    assert!(same.exists());
    assert!(report.exists());
    assert!(elsewhere.exists());
    assert!(table_gone(&state));
}

#[test]
fn absent_table_noop() {
    let (_guard, root) = testenv::lock();
    let (state, _, _) = testenv::harness();
    state.with_db(skwd_wall_core::db::drop_tinier_converts).unwrap();
    let dest = artifact(&root, "legacy/video-opt/orphan.ivf");
    assert!(table_gone(&state));
    retire_tinier(&state);
    assert!(dest.exists());
}

#[test]
fn failed_write_keeps_everything() {
    let (_guard, root) = testenv::lock();
    let cache = root.join("unwritable-cache");
    std::fs::create_dir_all(&cache).unwrap();
    testenv::write_config(json!({"paths": {"cache": cache}}));
    let (state, _, _) = testenv::harness();
    let source = root.join("videos/kept.mp4");
    let dest = artifact(&root, "legacy/video-opt/kept.ivf");
    legacy_rows(&state, &[(&source, &dest)]);
    let saved = root.join("x".repeat(255));
    std::fs::write(&saved, json!({"DP-1": {"type": "video", "path": dest}}).to_string()).unwrap();
    let file = skwd_wall_core::audio::state_path(&state.config().cache_dir());
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&saved, &file).unwrap();

    retire_tinier(&state);

    std::fs::remove_file(&file).unwrap();
    assert!(dest.exists());
    assert!(!table_gone(&state));
    state.with_db(skwd_wall_core::db::drop_tinier_converts).unwrap();
    testenv::write_config(json!({}));
}
