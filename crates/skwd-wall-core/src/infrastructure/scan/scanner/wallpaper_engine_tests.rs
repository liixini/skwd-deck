use super::*;

#[test]
fn missing_and_corrupt_previews_keep_valid_thumbnail_artifacts() {
    let root = tempfile::tempdir().unwrap();
    let item = root.path().join("item");
    let thumb = root.path().join("thumbs/item.webp");
    let small = root.path().join("small/item.webp");
    std::fs::create_dir_all(&item).unwrap();
    std::fs::write(item.join("project.json"), r#"{"type":"scene","preview":"thumbnail.png"}"#)
        .unwrap();
    for corrupt in [false, true] {
        if corrupt {
            std::fs::write(item.join("thumbnail.png"), b"invalid").unwrap();
        }
        let result = generate_thumbnails(&item, "42", None, &thumb, &small).unwrap();
        assert_eq!((result.width, result.height), (0, 0));
        assert!(artifacts_ready(&thumb));
        assert!(small.is_file());
        assert!(sources_unchanged(&item, None, &thumb));
    }
    image::RgbImage::from_pixel(8, 8, image::Rgb([255, 0, 0]))
        .save(item.join("thumbnail.png"))
        .unwrap();
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(2);
    std::fs::File::open(item.join("thumbnail.png"))
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(later))
        .unwrap();
    assert!(!sources_unchanged(&item, None, &thumb));
    let result = generate_thumbnails(&item, "42", None, &thumb, &small).unwrap();
    assert_eq!((result.width, result.height), (8, 8));
}
