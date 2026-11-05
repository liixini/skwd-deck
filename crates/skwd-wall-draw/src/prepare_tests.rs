use super::*;

#[test]
fn fit_keeps_small_sizes() {
    assert_eq!(fit([1920, 1080], INK_SIDE), [1920, 1080]);
}

#[test]
fn fit_caps_long_side() {
    assert_eq!(fit([5120, 2880], INK_SIDE), [2560, 1440]);
    assert_eq!(fit([4691, 7087], INK_SIDE), [1695, 2560]);
    assert_eq!(fit([2560, 1440], TIMING_SIDE), [1920, 1080]);
}

#[test]
fn layers_keep_faint_ink() {
    let mut strength = Plane::filled(8, 8, 0.0);
    strength.data[0] = 0.03;
    let (native, working) = ink_layers(&strength, [2, 2], 0.02);
    assert_eq!(native[0], 8);
    assert!(working.data[0] >= 0.02);
    assert_eq!(working.data[3], 0.0);
}

#[test]
fn manifest_passes_paper() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("wall.png");
    image::RgbImage::new(16, 8).save(&source).unwrap();
    let output = directory.path().join("drawing");
    std::fs::create_dir(&output).unwrap();
    let strokes = [crate::strokes::Stroke {
        points: vec![[1.0, 1.0], [6.0, 1.0]],
        width: 3.0,
        kind: crate::strokes::Kind::Trace,
    }];
    let events = [crate::plan::Event { stroke: 0, start: 0.0, end: 1.0 }];
    let farther = Plane::filled(8, 4, 0.5);
    Drawing {
        strokes: &strokes,
        events: &events,
        farther: &farther,
        matte: &Plane::filled(16, 8, 0.0),
        ink: &[0; 128],
        ink_size: [16, 8],
    }
    .write(&output)
    .unwrap();
    let sha256 = cache::hash_file(&source).unwrap();
    let written = manifest(origin(&source, &sha256).unwrap(), [16, 8], [8, 4], [4.0, 2.0]);
    assert_eq!(written.focus, [0.5, 0.5]);
    let artifact = output.join(cache::MANIFEST);
    std::fs::write(&artifact, serde_json::to_vec(&written).unwrap()).unwrap();
    assert!(paper_control::DrawingManifest::open(&artifact, &source).is_ok());
}

#[test]
fn vacant_dirs() {
    let directory = tempfile::tempdir().unwrap();
    assert!(vacant(&directory.path().join("missing")));
    assert!(vacant(directory.path()));
    std::fs::write(directory.path().join("note"), b"x").unwrap();
    assert!(!vacant(directory.path()));
}

#[test]
fn clear_empties_dir() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("drawing");
    clear(&output).unwrap();
    std::fs::create_dir_all(output.join("nested")).unwrap();
    std::fs::write(output.join("nested/inner"), b"x").unwrap();
    std::fs::write(output.join("note"), b"x").unwrap();
    clear(&output).unwrap();
    assert!(vacant(&output));
}

#[test]
fn clear_skips_links() {
    let directory = tempfile::tempdir().unwrap();
    let kept = directory.path().join("kept");
    std::fs::create_dir(&kept).unwrap();
    std::fs::write(kept.join("note"), b"x").unwrap();
    let output = directory.path().join("drawing");
    std::fs::create_dir(&output).unwrap();
    std::os::unix::fs::symlink(&kept, output.join("link")).unwrap();
    clear(&output).unwrap();
    assert!(vacant(&output));
    assert!(kept.join("note").exists());
}
