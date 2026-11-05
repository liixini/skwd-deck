use super::*;

#[test]
fn prune_keeps_current() {
    for source in ["steam", "wallhaven-full"] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join(source);
        std::fs::create_dir(&root).unwrap();
        let names = if source == "steam" {
            ["123-0000000000000001.jpg", "123-0000000000000002.jpg", "123-0000000000000003.jpg"]
        } else {
            ["first.jpg", "second.jpg", "third.jpg"]
        };
        let old = root.join(names[0]);
        let current = root.join(names[1]);
        let other = root.join(names[2]);
        let unrelated = root.join("readme.txt");
        let partial = root.join("pending.jpg.part");
        let legacy = root.join("123.jpg");
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            1,
            1,
            image::Rgba([1, 2, 3, 255]),
        ));
        let mut data = std::io::Cursor::new(Vec::new());
        image.write_to(&mut data, image::ImageFormat::Png).unwrap();
        for path in [&old, &current, &other, &legacy] {
            std::fs::write(path, data.get_ref()).unwrap();
        }
        for path in [&unrelated, &partial] {
            std::fs::write(path, [0_u8; 4]).unwrap();
        }
        prune_old(&current, (data.get_ref().len() * 2) as u64, 2).unwrap();
        assert!(current.exists());
        assert!(unrelated.exists());
        assert!(partial.exists());
        let remaining =
            usize::from(old.exists()) + usize::from(other.exists()) + usize::from(legacy.exists());
        assert_eq!(remaining, 1);
    }
}
