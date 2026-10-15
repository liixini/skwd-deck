use std::os::unix::ffi::OsStringExt;
use std::process::Command;

const WORKER_ARG: &str = "--lockscreen-render-worker";

#[test]
fn worker_preserves_byte_paths() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join(std::ffi::OsString::from_vec(b"state-\xff".to_vec()));
    let source = directory.path().join(std::ffi::OsString::from_vec(b"image-\xff.png".to_vec()));
    let data = directory.path().join("unused-data");
    let expected = image::RgbImage::from_pixel(3, 2, image::Rgb([12, 34, 56]));
    expected.save(&source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_skwd-walld"))
        .arg(WORKER_ARG)
        .arg(&source)
        .arg(&root)
        .env("XDG_DATA_HOME", &data)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let name = std::str::from_utf8(&output.stdout).unwrap().trim();
    let image = root.join(name);
    assert_eq!(image::open(image).unwrap().to_rgb8(), expected);
    assert!(!root.join("wallpaper.png").exists());
    assert!(!root.join("current").exists());
    assert!(!data.exists());
}

#[test]
fn invalid_source_preserves_pointers() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let source = root.join("broken.png");
    std::fs::write(&source, b"broken").unwrap();
    std::fs::write(root.join("wallpaper.png"), b"before").unwrap();
    std::fs::write(root.join("current"), b"before.png").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_skwd-walld"))
        .arg(WORKER_ARG)
        .arg(source)
        .arg(root)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(std::fs::read(root.join("wallpaper.png")).unwrap(), b"before");
    assert_eq!(std::fs::read(root.join("current")).unwrap(), b"before.png");
    let missing = Command::new(env!("CARGO_BIN_EXE_skwd-walld")).arg(WORKER_ARG).output().unwrap();
    assert!(!missing.status.success());
}
