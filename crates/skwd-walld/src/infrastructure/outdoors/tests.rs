use super::*;

struct Fixture {
    _dir: tempfile::TempDir,
    cache: String,
    source: PathBuf,
    thumb: PathBuf,
    near: PathBuf,
    far: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().to_string_lossy().into_owned();
        let source = entries(&cache).unwrap()[0].source(&cache);
        let thumb = source.with_extension(WEBP_EXTENSION);
        let (near, far) = skwd_wall_core::paths::thumbnail_blocks(&thumb.to_string_lossy());
        Self { _dir: dir, cache, source, thumb, near, far }
    }

    fn pending(&self) -> bool {
        missing_thumbs(&self.cache).iter().any(|(path, _)| *path == self.source)
    }

    fn thumb_of_item(&self) -> String {
        entries(&self.cache).unwrap()[0].item(&self.cache).thumb.unwrap()
    }
}

fn touch(paths: &[&PathBuf]) {
    for path in paths {
        std::fs::write(path, b"thumb").unwrap();
    }
}

#[test]
fn jpeg_before_webp() {
    let fixture = Fixture::new();
    assert!(fixture.pending());
    assert!(fixture.thumb_of_item().ends_with(THUMB_EXTENSION));
}

#[test]
fn partial_blocks_pending() {
    let fixture = Fixture::new();
    touch(&[&fixture.thumb, &fixture.near]);
    assert!(fixture.pending());
    assert!(fixture.thumb_of_item().ends_with(THUMB_EXTENSION));
}

#[test]
fn webp_when_fresh() {
    let fixture = Fixture::new();
    touch(&[&fixture.thumb, &fixture.near, &fixture.far]);
    assert!(!fixture.pending());
    assert_eq!(fixture.thumb_of_item(), fixture.thumb.to_string_lossy());
}

#[test]
fn stale_blocks_pending() {
    let fixture = Fixture::new();
    touch(&[&fixture.thumb, &fixture.near, &fixture.far]);
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
    std::fs::File::options()
        .write(true)
        .open(&fixture.source)
        .unwrap()
        .set_modified(later)
        .unwrap();
    assert!(fixture.pending());
}
