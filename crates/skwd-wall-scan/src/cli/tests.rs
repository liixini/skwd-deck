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
