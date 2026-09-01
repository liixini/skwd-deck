use super::*;

fn profile(lossless: bool) -> Profile {
    Profile {
        quality: if lossless { 100.0 } else { 82.0 },
        lossless,
        min_savings_percent: 15,
        min_psnr_db: 30.0,
        max_width: 2560,
        max_height: 1440,
    }
}

#[test]
fn portrait_bounds() {
    let profile = profile(false);
    assert_eq!(profile.bounds_for(3000, 2000), (2560, 1440));
    assert_eq!(profile.bounds_for(2000, 3000), (1440, 2560));
}

#[test]
fn source_scan_skips_webp() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("nested")).unwrap();
    std::fs::write(directory.path().join("a.png"), b"x").unwrap();
    std::fs::write(directory.path().join("nested/b.JPG"), b"x").unwrap();
    std::fs::write(directory.path().join("done.webp"), b"x").unwrap();
    std::fs::create_dir_all(directory.path().join(".skwd-wall-v2/trash/images")).unwrap();
    std::fs::write(directory.path().join(".skwd-wall-v2/trash/images/hidden.png"), b"x").unwrap();
    let sources = image_sources(directory.path());
    assert_eq!(sources.len(), 2);
    assert!(sources.iter().any(|path| path.ends_with("a.png")));
    assert!(sources.iter().any(|path| path.ends_with("nested/b.JPG")));
}

#[test]
fn delta_candidates() {
    let directory = tempfile::tempdir().unwrap();
    let image = directory.path().join("new.png");
    let webp = directory.path().join("already.webp");
    let internal = directory.path().join(".skwd-wall-v2/work/images/temp.png");
    std::fs::create_dir_all(internal.parent().unwrap()).unwrap();
    std::fs::write(&image, b"x").unwrap();
    std::fs::write(&webp, b"x").unwrap();
    std::fs::write(&internal, b"x").unwrap();
    let outside = directory.path().parent().unwrap().join("outside.png");
    let paths = BTreeSet::from([image.clone(), webp, internal, outside]);
    assert_eq!(image_sources_from(directory.path(), &paths), vec![image]);
}

#[test]
fn static_stages_output() {
    let directory = tempfile::tempdir().unwrap();
    let wallpaper_dir = directory.path().join("walls");
    let source = wallpaper_dir.join("nested/large.png");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    let image = image::RgbaImage::from_fn(320, 180, |x, y| {
        image::Rgba([(x % 255) as u8, (y % 255) as u8, ((x + y) % 255) as u8, 255])
    });
    image.save(&source).unwrap();
    let options = Options {
        wallpaper_dir: wallpaper_dir.clone(),
        trash_dir: directory.path().join("trash"),
        legacy_trash_dir: directory.path().join("legacy-trash"),
        work_dir: wallpaper_dir.join(".skwd-wall-v2/work/images"),
        profile: profile(false),
        profile_key: "light@1080p".into(),
        retention_days: 7,
        clean_trash: false,
    };
    let result = match optimize_one(&source, &options).unwrap() {
        Optimized::Changed(result) => result,
        Optimized::Skipped(reason) => panic!("skipped: {reason}"),
    };
    assert_eq!(result.old_name, "nested/large.png");
    assert_eq!(result.new_name, "nested/large.webp");
    assert!(wallpaper_dir.join("nested/large.webp").is_file());
    assert!(source.exists());
    assert!(!options.trash_dir.join("nested/large.png").exists());
}

#[test]
fn trash_cleanup_obeys_retention() {
    let directory = tempfile::tempdir().unwrap();
    let old = directory.path().join("old.png");
    std::fs::write(&old, b"x").unwrap();
    assert_eq!(clean_trash(directory.path(), 0), CleanupStats { files: 1, bytes: 1 });
    assert!(!old.exists());
}

#[test]
fn touch_resets_retention() {
    let directory = tempfile::tempdir().unwrap();
    let old = directory.path().join("old.png");
    std::fs::write(&old, b"x").unwrap();
    let file = std::fs::OpenOptions::new().write(true).open(&old).unwrap();
    file.set_times(std::fs::FileTimes::new().set_modified(SystemTime::UNIX_EPOCH)).unwrap();
    touch_modified_now(&old);
    assert_eq!(clean_trash(directory.path(), 1), CleanupStats::default());
    assert!(old.exists());
}

#[test]
fn savings_gate() {
    let mut profile = profile(false);
    profile.min_savings_percent = 10;
    assert!(!profile.saves_enough(1_000, 901));
    assert!(profile.saves_enough(1_000, 900));
}

#[test]
fn lossless_roundtrip() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.png");
    let destination = directory.path().join("result.webp");
    let original = image::RgbaImage::from_fn(96, 64, |x, y| {
        image::Rgba([(x * 2) as u8, (y * 3) as u8, (x + y) as u8, ((x + y) % 255) as u8])
    });
    original.save(&source).unwrap();
    let EncodeAttempt::Written { width, height } =
        encode_static(&source, &destination, profile(true)).unwrap()
    else {
        panic!("lossless encode");
    };
    assert_eq!((width, height), original.dimensions());
    assert_eq!(image::open(destination).unwrap().to_rgba8(), original);
}

#[test]
fn animated_gif_detected() {
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, Frame};

    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.gif");
    let file = std::fs::File::create(&source).unwrap();
    let mut encoder = GifEncoder::new(file);
    encoder.set_repeat(Repeat::Infinite).unwrap();
    let frames = (0..3).map(|index| {
        let pixels = image::RgbaImage::from_pixel(
            64,
            48,
            image::Rgba([index * 80, 255 - index * 80, 40, 255]),
        );
        Frame::from_parts(pixels, 0, 0, Delay::from_numer_denom_ms(100, 1))
    });
    encoder.encode_frames(frames).unwrap();
    drop(encoder);

    assert!(gif_is_animated(&source).unwrap());
}
