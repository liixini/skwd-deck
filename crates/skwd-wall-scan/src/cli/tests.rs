#![cfg(test)]

use super::*;

fn arguments(values: &[&str]) -> Vec<String> {
    values.iter().map(ToString::to_string).collect()
}

#[test]
fn version_flags() {
    assert!(matches!(parse(&arguments(&["scan", "--version"])), Command::Version));
    assert!(matches!(parse(&arguments(&["scan", "-V"])), Command::Version));
}

#[test]
fn gpu_worker_cache() {
    for (values, expected) in [
        (vec!["scan", skwd_wall_core::blocks::GPU_WORKER_FLAG, "/cache/gpu"], "/cache/gpu"),
        (vec!["scan", skwd_wall_core::blocks::GPU_WORKER_FLAG], ""),
    ] {
        let Command::GpuWorker { cache } = parse(&arguments(&values)) else {
            panic!("expected GPU worker");
        };
        assert_eq!(cache, expected);
    }
}

#[test]
fn preview_positionals() {
    let Command::Preview { key, video } =
        parse(&arguments(&["scan", "--preview", "video:key", "/tmp/a.mp4"]))
    else {
        panic!("expected preview command");
    };
    assert_eq!(key, "video:key");
    assert_eq!(video, "/tmp/a.mp4");
}

#[test]
fn tall_positionals() {
    let Command::Tall { source, thumb, video } =
        parse(&arguments(&["scan", "--tall", "/tmp/a.png", "/tmp/thumbs/a.webp"]))
    else {
        panic!("expected tall command");
    };
    assert_eq!(
        (source.as_str(), thumb.as_str(), video),
        ("/tmp/a.png", "/tmp/thumbs/a.webp", false)
    );
    let Command::Tall { video, .. } =
        parse(&arguments(&["scan", "--tall", "/tmp/a.mp4", "/tmp/t.webp", "--video"]))
    else {
        panic!("expected tall command");
    };
    assert!(video);
}

#[test]
fn ansi_options() {
    let Command::Ansi16 { dark, auto, variant, .. } =
        parse(&arguments(&["scan", "--ansi16", "a.png", "--light", "--auto", "--variant", "soft"]))
    else {
        panic!("expected ansi command");
    };
    assert!(!dark);
    assert!(auto);
    assert_eq!(variant, "soft");
}

#[test]
fn scene_probe_dir() {
    let Command::SceneProbe { dir } =
        parse(&arguments(&["scan", "--scene-probe", "/workshop/123"]))
    else {
        panic!("expected scene probe command");
    };
    assert_eq!(dir, "/workshop/123");
}

#[test]
fn correlated_paths_keep_request_id_out_of_paths() {
    let Command::Paths { changed, request_id } = parse(&arguments(&[
        "scan",
        "--scan-request-id",
        "watch-17",
        "--paths",
        "/walls/a.png",
        "/walls/b.png",
    ])) else {
        panic!("expected paths command");
    };
    assert_eq!(changed, [PathBuf::from("/walls/a.png"), PathBuf::from("/walls/b.png")]);
    assert_eq!(request_id.as_deref(), Some("watch-17"));
}

#[test]
fn full_scan_accepts_correlation_id() {
    let Command::FullScan { request_id } =
        parse(&arguments(&["scan", "--scan-request-id", "watch-18"]))
    else {
        panic!("expected full scan command");
    };
    assert_eq!(request_id.as_deref(), Some("watch-18"));
}

#[test]
fn studio_area() {
    let area = |text: &str| match parse(&arguments(&["scan", "--studio", "a.png", "--area", text]))
    {
        Command::Studio { image, area } => {
            assert_eq!(image, "a.png");
            area
        }
        _ => panic!("not a studio command"),
    };
    let picked = area("0.1, 0.2,0.6,0.9").expect("area");
    assert_eq!((picked.left, picked.top, picked.right, picked.bottom), (0.1, 0.2, 0.6, 0.9));
    assert!(area("0.6,0.2,0.1,0.9").is_none());
    assert!(area("0.1,0.2,0.6").is_none());
    assert!(area("a,b,c,d").is_none());
}

#[test]
fn thumbs_pairs() {
    let Command::Thumbs { jobs } =
        parse(&arguments(&["scan", "--thumbs", "/c/a.jpg", "/c/a.webp", "/c/b.jpg", "/c/b.webp"]))
    else {
        panic!("expected thumbs command");
    };
    assert_eq!(
        jobs,
        [
            (PathBuf::from("/c/a.jpg"), PathBuf::from("/c/a.webp")),
            (PathBuf::from("/c/b.jpg"), PathBuf::from("/c/b.webp")),
        ]
    );
}
