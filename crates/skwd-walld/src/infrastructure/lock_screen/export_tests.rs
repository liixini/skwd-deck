use super::*;
use serde_json::json;

#[test]
fn dynamic_needs_poster() {
    let config = json!({"lockScreens":{"mode":"follow"}});
    let current = LockScreenCurrent {
        kind: wall_proto::kind::VIDEO,
        path: "movie.mp4",
        we_id: "",
        poster: "movie.mp4",
    };
    assert!(source(&config, Some(&current)).is_err());
}

#[test]
fn export_survives_cache_removal() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("poster.png");
    image::RgbImage::new(2, 2).save(&input).unwrap();
    let root = temp.path().join("state");
    let config = json!({"lockScreens":{"mode":"static","image":input}});
    let exported = render(&config, None, &root).unwrap();
    std::fs::remove_file(input).unwrap();
    assert_eq!(previous(&root).unwrap(), Some(exported.clone()));
    assert!(image::open(exported).is_ok());
}

#[test]
fn changed_images_change_paths() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("image.png");
    let root = temp.path().join("state");
    image::RgbImage::new(2, 2).save(&input).unwrap();
    let config = json!({"lockScreens":{"mode":"static","image":input}});
    let first = render(&config, None, &root).unwrap();
    assert_eq!(render(&config, None, &root).unwrap(), first);
    image::RgbImage::from_pixel(2, 2, image::Rgb([255, 0, 0])).save(&input).unwrap();
    assert_ne!(render(&config, None, &root).unwrap(), first);
}

#[test]
fn invalid_image_preserves_export() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("image.png");
    let root = temp.path().join("state");
    image::RgbImage::new(2, 2).save(&input).unwrap();
    let config = json!({"lockScreens":{"mode":"static","image":input}});
    let first = render(&config, None, &root).unwrap();
    std::fs::write(input, "not an image").unwrap();
    assert!(render(&config, None, &root).is_err());
    assert_eq!(previous(&root).unwrap(), Some(first));
}

#[test]
fn sddm_requires_shared_directory() {
    let temp = tempfile::tempdir().unwrap();
    let private = temp.path().join("private");
    std::fs::create_dir(&private).unwrap();
    std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o700)).unwrap();
    let config = json!({"lockScreens":{"sddmImage":private.join("wallpaper.png")}});
    assert!(export_sddm(&config, Path::new("unused"), temp.path()).is_err());
    assert!(!private.join("wallpaper.png").exists());
}

#[test]
fn pruning_keeps_receipt_images() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let names = [
        "wallpaper-0000000000000001.png",
        "wallpaper-0000000000000002.png",
        "wallpaper-0000000000000003.png",
        "wallpaper-0000000000000004.png",
        "user.png",
    ];
    for name in names {
        std::fs::write(root.join(name), b"image").unwrap();
    }
    std::fs::write(root.join("dms.json"), json!({"after":root.join(names[2])}).to_string())
        .unwrap();
    prune(root, &root.join(names[0]), Some(&root.join(names[1]))).unwrap();
    for name in [names[0], names[1], names[2], names[4]] {
        assert!(root.join(name).exists());
    }
    assert!(!root.join(names[3]).exists());
}

#[test]
fn sddm_requires_png_extension() {
    let temp = tempfile::tempdir().unwrap();
    let destination = temp.path().join("theme.conf");
    std::fs::write(&destination, "keep").unwrap();
    let config = json!({"lockScreens":{"sddmImage":destination}});
    assert!(export_sddm(&config, Path::new("unused"), temp.path()).is_err());
    assert_eq!(std::fs::read_to_string(destination).unwrap(), "keep");
}

#[test]
fn unselected_targets_skip_export() {
    let (_guard, _) = crate::testenv::lock();
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("source.png");
    image::RgbImage::new(2, 2).save(&input).unwrap();
    crate::testenv::write_config(json!({"lockScreens":{"mode":"static","image":input}}));
    let (state, _, _) = crate::testenv::harness();
    let root = skwd_wall_core::paths::data_dir().join(lock_screens::DIRECTORY);
    assert!(!root.exists());
    super::super::sync_images(&state, None);
    assert!(!root.exists());
}

#[test]
fn sddm_preserves_unowned_image() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("wallpaper.png");
    image::RgbImage::new(2, 2).save(&path).unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(write_sddm(&path, &path, temp.path()).is_err());
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn sddm_preserves_external_edits() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("wallpaper.png");
    let input = temp.path().join("source.png");
    std::fs::write(&input, b"first").unwrap();
    write_sddm(&path, &input, temp.path()).unwrap();
    let second = temp.path().join("second.png");
    std::fs::write(&second, b"second").unwrap();
    write_sddm(&path, &second, temp.path()).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"second");
    std::fs::write(&path, b"user").unwrap();
    assert!(write_sddm(&path, &input, temp.path()).is_err());
    assert_eq!(std::fs::read(path).unwrap(), b"user");
}

#[test]
fn sddm_disable_rollback() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("wallpaper.png");
    let input = temp.path().join("source.png");
    std::fs::write(&input, b"owned").unwrap();
    write_sddm(&path, &input, temp.path()).unwrap();
    let config = json!({"lockScreens":{"sddmImage":path}});
    let snapshot = sddm_snapshot(&config, temp.path()).unwrap();
    remove_sddm(&config, temp.path()).unwrap();
    assert!(!path.exists());
    assert!(!sddm_record(&path, temp.path()).exists());
    snapshot.restore(temp.path()).unwrap();
    assert_eq!(std::fs::read(path).unwrap(), b"owned");
}

#[test]
fn sddm_external_image() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("wallpaper.png");
    let input = temp.path().join("source.png");
    std::fs::write(&input, b"owned").unwrap();
    write_sddm(&path, &input, temp.path()).unwrap();
    std::fs::write(&path, b"user").unwrap();
    let config = json!({"lockScreens":{"sddmImage":path}});
    assert!(remove_sddm(&config, temp.path()).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"user");
    assert!(sddm_record(&path, temp.path()).exists());
}
