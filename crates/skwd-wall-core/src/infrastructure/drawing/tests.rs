use super::*;
use serde_json::json;

fn fixture() -> (tempfile::TempDir, Config, PathBuf, CacheEntry) {
    let directory = tempfile::tempdir().unwrap();
    let config =
        Config::from_root(json!({"paths":{"cache":directory.path(),"wallpaper":directory.path()}}));
    let source = directory.path().join("wallpaper.png");
    std::fs::write(&source, b"original").unwrap();
    let entry =
        CacheEntry { artifact: digest(b"artifact"), pack: digest(b"pack"), recipe: RECIPE.into() };
    let artifact = artifacts(&config).join(&entry.artifact);
    std::fs::create_dir_all(&artifact).unwrap();
    std::fs::write(artifact.join("drawing.png"), b"texture").unwrap();
    std::fs::write(artifact.join("strokes.bin"), b"strokes").unwrap();
    let metadata = source.metadata().unwrap();
    let manifest = json!({
        "version":3,"source":{"path":source,"size":metadata.len(),
        "mtime_ns":metadata.modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),"sha256":hash_file(&source).unwrap()},
        "width":10,"height":10,"texture":"drawing.png","strokes":"strokes.bin","timing_width":8,"timing_height":8,"focus":[0.5,0.5]
    });
    std::fs::write(artifact.join(MANIFEST), serde_json::to_vec(&manifest).unwrap()).unwrap();
    publish(&config, &source, &entry).unwrap();
    (directory, config, source, entry)
}

#[test]
fn cached_without_models() {
    let (_directory, config, source, entry) = fixture();
    assert_eq!(
        lookup(&config, &source).unwrap(),
        artifacts(&config).join(entry.artifact).join(MANIFEST)
    );
}

#[test]
fn rejects_changed_source() {
    let (_directory, config, source, _) = fixture();
    std::fs::write(&source, b"replacement wallpaper").unwrap();
    assert!(lookup(&config, &source).is_err());
}

#[test]
fn rejects_recipe_change() {
    let (_directory, config, source, mut entry) = fixture();
    entry.recipe = "old".into();
    publish(&config, &source, &entry).unwrap();
    assert!(lookup(&config, &source).is_err());
}

#[test]
fn rejects_pack_change() {
    let (_directory, config, source, _) = fixture();
    std::fs::write(root(&config).join(PACK_RECORD), br#"{"fingerprint":"replacement"}"#).unwrap();
    assert!(lookup(&config, &source).is_err());
    std::fs::remove_file(root(&config).join(PACK_RECORD)).unwrap();
    assert!(lookup(&config, &source).is_ok());
}

#[test]
fn clear_keeps_processing() {
    let (_directory, config, source, _) = fixture();
    let pack = root(&config).join(PACK_RECORD);
    std::fs::write(&pack, b"processing files").unwrap();
    clear(&config).unwrap();
    assert!(pack.is_file());
    assert!(!artifacts(&config).exists());
    assert!(!index_path(&config, &source).unwrap().exists());
}

#[test]
fn rejects_cache_escape() {
    let (_directory, config, source, mut entry) = fixture();
    entry.artifact = "../outside".into();
    assert!(publish(&config, &source, &entry).is_err());
}

#[test]
fn counts_only_owned_files() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("owned"), b"one").unwrap();
    std::fs::write(outside.path().join("shared"), b"other").unwrap();
    std::os::unix::fs::symlink(outside.path(), directory.path().join("link")).unwrap();
    assert_eq!(directory_bytes(directory.path()).unwrap(), 3);
}

#[test]
fn media_reference_invalidates() {
    let (_directory, config, source, entry) = fixture();
    let reference = references(&config).join("poster.png");
    std::fs::create_dir_all(reference.parent().unwrap()).unwrap();
    std::fs::copy(&source, &reference).unwrap();
    let manifest = artifacts(&config).join(&entry.artifact).join(MANIFEST);
    let metadata = reference.metadata().unwrap();
    let body = json!({
        "version":3,
        "source":{"path":reference,"size":metadata.len(),
            "mtime_ns":metadata.modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),
            "sha256":hash_file(&reference).unwrap()},
        "width":10,"height":10,"texture":"drawing.png","strokes":"strokes.bin","timing_width":8,"timing_height":8,"focus":[0.5,0.5]
    });
    std::fs::write(&manifest, serde_json::to_vec(&body).unwrap()).unwrap();
    let media = MediaEntry {
        cache: entry,
        signature: digest(b"media-v1"),
        reference: reference.clone(),
        source: None,
    };
    publish_media(&config, &source, &media).unwrap();
    assert_eq!(lookup_media(&config, &source, &media.signature).unwrap().0, manifest);
    assert!(lookup_media(&config, &source, &digest(b"changed")).is_err());
    std::fs::write(&reference, b"new frame").unwrap();
    assert!(lookup_media(&config, &source, &media.signature).is_err());
}

#[test]
#[cfg(feature = "daemon")]
fn we_settings_invalidate() {
    let root = tempfile::tempdir().unwrap();
    let item = root.path().join("42");
    std::fs::create_dir(&item).unwrap();
    std::fs::write(item.join("project.json"), br#"{"type":"scene"}"#).unwrap();
    std::fs::write(item.join("scene.pkg"), b"scene").unwrap();
    let state = crate::state::WallState::test_new(json!({
        "paths":{"steamWorkshop":root.path()}
    }));
    let original = media_signature(&state, "we:42", &item).unwrap();
    state
        .with_db(|connection| {
            crate::db::set_we_property(connection, "42", "tint", Some(&json!("1 0 0")))
        })
        .unwrap();
    let changed = media_signature(&state, "we:42", &item).unwrap();
    assert_ne!(original, changed);
    std::fs::write(item.join("scene.pkg"), b"changed scene").unwrap();
    assert_ne!(changed, media_signature(&state, "we:42", &item).unwrap());
}

#[test]
#[cfg(feature = "daemon")]
fn we_dependency_invalidates() {
    let root = tempfile::tempdir().unwrap();
    let base = root.path().join("42");
    let preset = root.path().join("43");
    std::fs::create_dir(&base).unwrap();
    std::fs::create_dir(&preset).unwrap();
    std::fs::write(base.join("project.json"), br#"{"type":"scene"}"#).unwrap();
    std::fs::write(base.join("scene.pkg"), b"scene").unwrap();
    std::fs::write(preset.join("project.json"), br#"{"dependency":"42","preset":{}}"#).unwrap();
    let state = crate::state::WallState::test_new(json!({"paths":{"steamWorkshop":root.path()}}));
    let original = media_signature(&state, "we:43", &preset).unwrap();
    std::fs::write(base.join("scene.pkg"), b"changed scene").unwrap();
    assert_ne!(original, media_signature(&state, "we:43", &preset).unwrap());
}

#[test]
fn recorded_checks_pack() {
    let (_directory, config, source, entry) = fixture();
    assert!(recorded(&config, &source, None, &entry.pack));
    assert!(!recorded(&config, &source, None, &digest(b"other pack")));
    assert!(!recorded(&config, &source, Some("signature"), &entry.pack));
    std::fs::write(&source, b"edited").unwrap();
    assert!(!recorded(&config, &source, None, &entry.pack));
}

#[test]
fn republish_drops_old_artifact() {
    let (_directory, config, source, entry) = fixture();
    let next = CacheEntry { artifact: digest(b"next"), pack: entry.pack, recipe: entry.recipe };
    publish(&config, &source, &next).unwrap();
    assert!(!artifacts(&config).join(entry.artifact).exists());
}

#[test]
fn media_recorded_signature() {
    let (_directory, config, source, entry) = fixture();
    let pack = entry.pack.clone();
    let reference = references(&config).join("poster.png");
    std::fs::create_dir_all(reference.parent().unwrap()).unwrap();
    std::fs::copy(&source, &reference).unwrap();
    let signature = digest(b"media-v1");
    let media = MediaEntry {
        cache: entry,
        signature: signature.clone(),
        reference: reference.clone(),
        source: Some(source.clone()),
    };
    publish_media(&config, &source, &media).unwrap();
    assert!(recorded(&config, &source, Some(&signature), &pack));
    assert!(!recorded(&config, &source, Some(&digest(b"changed")), &pack));
    std::fs::remove_file(reference).unwrap();
    assert!(!recorded(&config, &source, Some(&signature), &pack));
}

#[test]
fn prune_keeps_unmounted() {
    let (directory, config, source, entry) = fixture();
    let mounted = directory.path().join("library");
    std::fs::create_dir(&mounted).unwrap();
    let moved = mounted.join("wallpaper.png");
    std::fs::rename(&source, &moved).unwrap();
    let manifest = artifacts(&config).join(&entry.artifact).join(MANIFEST);
    let mut body: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
    body["source"]["path"] = json!(moved);
    std::fs::write(&manifest, serde_json::to_vec(&body).unwrap()).unwrap();
    std::fs::remove_dir_all(&mounted).unwrap();
    prune(&config).unwrap();
    assert!(artifacts(&config).join(&entry.artifact).exists());
}

#[test]
fn prune_drops_orphans() {
    let (_directory, config, source, entry) = fixture();
    let orphan = artifacts(&config).join(digest(b"orphan"));
    std::fs::create_dir_all(&orphan).unwrap();
    let leftover = root(&config).join(WORK_DIRECTORY).join("job-crashed");
    std::fs::create_dir_all(&leftover).unwrap();
    let stray = references(&config).join("stray.png");
    std::fs::create_dir_all(stray.parent().unwrap()).unwrap();
    std::fs::write(&stray, b"frame").unwrap();
    prune(&config).unwrap();
    assert!(!orphan.exists() && !stray.exists() && !leftover.exists());
    assert!(artifacts(&config).join(&entry.artifact).exists());
    let record = index_path(&config, &source).unwrap();
    std::fs::remove_file(&source).unwrap();
    prune(&config).unwrap();
    assert!(!artifacts(&config).join(&entry.artifact).exists());
    assert!(!record.exists());
}

#[test]
fn prune_skips_empty_library() {
    let (directory, _config, source, entry) = fixture();
    let library = directory.path().join("mount");
    std::fs::create_dir(&library).unwrap();
    let config = Config::from_root(
        json!({"paths":{"cache":directory.path(),"wallpaper":library.display().to_string()}}),
    );
    std::fs::remove_file(&source).unwrap();
    prune(&config).unwrap();
    assert!(artifacts(&config).join(&entry.artifact).exists());
}

#[test]
fn prune_drops_stale_recipe() {
    let (_directory, config, source, mut entry) = fixture();
    entry.recipe = "old".into();
    publish(&config, &source, &entry).unwrap();
    let record = index_path(&config, &source).unwrap();
    prune(&config).unwrap();
    assert!(!record.exists());
    assert!(!artifacts(&config).join(&entry.artifact).exists());
}

fn poster(config: &Config, source: &Path, cache: CacheEntry) -> MediaEntry {
    let reference = references(config).join("poster.png");
    std::fs::create_dir_all(reference.parent().unwrap()).unwrap();
    std::fs::copy(source, &reference).unwrap();
    let media = MediaEntry {
        cache,
        signature: digest(b"media-v1"),
        reference,
        source: Some(source.to_path_buf()),
    };
    publish_media(config, source, &media).unwrap();
    media
}

fn register(config: &Config, fingerprint: &str) -> PathBuf {
    let model = root(config).join("model.onnx");
    std::fs::write(&model, b"weights").unwrap();
    let pack = json!({"fingerprint":fingerprint,"inventory":[PackStamp::capture(&model).unwrap()]});
    std::fs::write(root(config).join(PACK_RECORD), serde_json::to_vec(&pack).unwrap()).unwrap();
    model
}

#[test]
fn prune_drops_stale_media() {
    let (_directory, config, source, entry) = fixture();
    let media = poster(&config, &source, CacheEntry { recipe: "old".into(), ..entry });
    prune(&config).unwrap();
    assert!(!media_path(&config, &source).unwrap().exists());
    assert!(!media.reference.exists());
}

#[test]
fn prune_drops_stale_pack() {
    let (_directory, config, source, entry) = fixture();
    let record = index_path(&config, &source).unwrap();
    let artifact = artifacts(&config).join(&entry.artifact);
    let pack = entry.pack.clone();
    let media = poster(&config, &source, entry);
    register(&config, &pack);
    prune(&config).unwrap();
    assert!(record.exists() && artifact.exists() && media.reference.exists());
    register(&config, &digest(b"replacement"));
    prune(&config).unwrap();
    assert!(!record.exists() && !artifact.exists());
    assert!(!media_path(&config, &source).unwrap().exists());
    assert!(!media.reference.exists());
}

#[test]
fn prune_keeps_broken_pack() {
    let (_directory, config, source, entry) = fixture();
    let model = register(&config, &digest(b"replacement"));
    std::fs::write(&model, b"retrained").unwrap();
    assert!(validate_processing(&config).is_err());
    prune(&config).unwrap();
    assert!(index_path(&config, &source).unwrap().exists());
    assert!(artifacts(&config).join(&entry.artifact).exists());
}

#[test]
fn publish_media_swaps_extension() {
    let (_directory, config, source, entry) = fixture();
    let CacheEntry { artifact, pack, recipe } = entry;
    let jpg = references(&config).join("poster.jpg");
    std::fs::create_dir_all(jpg.parent().unwrap()).unwrap();
    std::fs::copy(&source, &jpg).unwrap();
    let signature = digest(b"media-v1");
    let first = MediaEntry {
        cache: CacheEntry {
            artifact: artifact.clone(),
            pack: pack.clone(),
            recipe: recipe.clone(),
        },
        signature: signature.clone(),
        reference: jpg.clone(),
        source: Some(source.clone()),
    };
    publish_media(&config, &source, &first).unwrap();
    let webp = references(&config).join("poster.webp");
    std::fs::copy(&source, &webp).unwrap();
    let second = MediaEntry {
        cache: CacheEntry { artifact, pack, recipe },
        signature,
        reference: webp.clone(),
        source: Some(source.clone()),
    };
    publish_media(&config, &source, &second).unwrap();
    assert!(!jpg.exists());
    assert!(webp.exists());
    let record: MediaEntry = read_record(&media_path(&config, &source).unwrap()).unwrap();
    assert_eq!(record.reference, webp);
}

#[test]
fn prune_checks_each_root() {
    let (directory, _config, _source, entry) = fixture();
    let videos = directory.path().join("videos");
    std::fs::create_dir(&videos).unwrap();
    let config = Config::from_root(json!({"paths":{
        "cache":directory.path(),"wallpaper":directory.path(),"videoWallpaper":videos
    }}));
    let reference = references(&config).join("frame.png");
    std::fs::create_dir_all(reference.parent().unwrap()).unwrap();
    std::fs::write(&reference, b"frame").unwrap();
    let clip = videos.join("clip.mp4");
    let media = MediaEntry {
        cache: CacheEntry { artifact: digest(b"clip"), ..entry },
        signature: digest(b"clip-v1"),
        reference: reference.clone(),
        source: Some(clip.clone()),
    };
    std::fs::write(&clip, b"video").unwrap();
    publish_media(&config, &clip, &media).unwrap();
    std::fs::remove_file(&clip).unwrap();
    prune(&config).unwrap();
    assert!(reference.exists());
}
