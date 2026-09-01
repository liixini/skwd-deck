use std::io::Write;
use std::process::{Command, Stdio};

fn assert_version(flag: &str) {
    let output = Command::new(env!("CARGO_BIN_EXE_skwd-walld"))
        .arg(flag)
        .output()
        .expect("run skwd-walld version command");

    assert!(output.status.success(), "exit: {output:?}");
    assert_eq!(
        std::str::from_utf8(&output.stdout).expect("version output is UTF-8"),
        format!("skwd-walld {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert!(output.stderr.is_empty(), "stderr: {output:?}");
}

#[test]
fn long_version_clean() {
    assert_version("--version");
}

#[test]
fn short_version_clean() {
    assert_version("-V");
}

#[test]
fn image_optimize_worker() {
    let root = tempfile::tempdir().unwrap();
    let config_dir = root.path().join("config/skwd-wall-v2");
    let data_dir = root.path().join("data");
    let cache_dir = root.path().join("cache");
    let runtime_dir = root.path().join("run");
    let wallpaper_dir = root.path().join("walls");
    for path in [&config_dir, &data_dir, &cache_dir, &runtime_dir, &wallpaper_dir] {
        std::fs::create_dir_all(path).unwrap();
    }

    let source = wallpaper_dir.join("worker-fixture.png");
    let pixels = image::RgbaImage::from_fn(640, 360, |x, y| {
        image::Rgba([(x % 255) as u8, (y % 255) as u8, ((x + y) % 255) as u8, 255])
    });
    pixels.save(&source).unwrap();
    std::fs::write(
        config_dir.join("config.json"),
        serde_json::json!({
            "paths": { "wallpaper": wallpaper_dir },
            "performance": {
                "imageOptimizePreset": "light",
                "imageOptimizeResolution": "1080p"
            }
        })
        .to_string(),
    )
    .unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_skwd-walld"))
        .arg("--image-optimize-worker")
        .env("HOME", root.path())
        .env("XDG_CONFIG_HOME", root.path().join("config"))
        .env("XDG_DATA_HOME", &data_dir)
        .env("XDG_CACHE_HOME", &cache_dir)
        .env("XDG_RUNTIME_DIR", &runtime_dir)
        .env_remove("SKWD_WALL_CONFIG")
        .env_remove("SKWD_WALL_V2_CACHE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            serde_json::json!({
                "full_scan": true,
                "force": true,
                "clean_trash": false,
                "paths": []
            })
            .to_string()
            .as_bytes(),
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "worker failed: {}", String::from_utf8_lossy(&output.stderr));
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["status"]["optimized"], serde_json::json!(1));
    assert!(result["status"]["errors"].as_u64().unwrap_or(1) == 0);
    assert_eq!(result["scan_paths"].as_array().map(Vec::len), Some(1));
    assert!(!source.exists());
    assert!(wallpaper_dir.join("worker-fixture.webp").is_file());
    assert!(wallpaper_dir.join(".skwd-wall-v2/trash/images/worker-fixture.png").is_file());
}
