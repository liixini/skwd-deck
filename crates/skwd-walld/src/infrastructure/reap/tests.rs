#![cfg(test)]

use crate::testenv::tmp;

static DIRS: std::sync::LazyLock<Vec<String>> = std::sync::LazyLock::new(|| {
    vec![
        String::from("/home/u/.cache/skwd-wall-v2/video-opt"),
        String::from("/home/u/wallpaper-videos"),
    ]
});

#[test]
fn reap_stale_renderers() {
    let argv =
        |parts: &[&str]| parts.iter().map(std::string::ToString::to_string).collect::<Vec<_>>();
    let wallds = [4321];
    let plasmas = [1051];
    assert!(super::is_reapable_stale_renderer(
        &argv(&["/usr/bin/skwd-wall-still", "--out", "DP-1"]),
        1,
        &wallds,
        &plasmas,
        &DIRS
    ));
    assert!(super::is_reapable_stale_renderer(
        &argv(&["skwd-wall-still"]),
        1,
        &wallds,
        &plasmas,
        &DIRS
    ));
    assert!(super::is_reapable_stale_renderer(
        &argv(&["skwd-wall-vk"]),
        1,
        &wallds,
        &plasmas,
        &DIRS
    ));
    assert!(super::is_reapable_stale_renderer(
        &argv(&["skwd-wall-vk"]),
        1051,
        &wallds,
        &plasmas,
        &DIRS
    ));
    assert!(!super::is_reapable_stale_renderer(
        &argv(&["skwd-wall-vk", "--video-stream", "/v/a.mp4"]),
        1051,
        &wallds,
        &plasmas,
        &DIRS
    ));
    assert!(super::is_reapable_stale_renderer(
        &argv(&["skwd-wall-vk", "--video-stream", "/v/a.mp4"]),
        1,
        &wallds,
        &plasmas,
        &DIRS
    ));
    assert!(super::is_reapable_stale_renderer(
        &argv(&["/opt/bin/linux-wallpaperengine", "12345"]),
        1,
        &wallds,
        &plasmas,
        &DIRS
    ));
    assert!(!super::is_reapable_stale_renderer(
        &argv(&["/usr/bin/skwd-wall-still", "--out", "DP-1"]),
        4321,
        &wallds,
        &plasmas,
        &DIRS
    ));
    assert!(!super::is_reapable_stale_renderer(
        &argv(&["skwd-shell-helper"]),
        1,
        &wallds,
        &plasmas,
        &DIRS
    ));
    assert!(super::is_reapable_stale_renderer(
        &argv(&["skwd-wall-scan", "--analyze"]),
        1,
        &wallds,
        &plasmas,
        &DIRS
    ));
    assert!(!super::is_reapable_stale_renderer(
        &argv(&["skwd-wall-scan"]),
        1,
        &wallds,
        &plasmas,
        &DIRS
    ));
    assert!(!super::is_reapable_stale_renderer(&[], 1, &wallds, &plasmas, &DIRS));
}

#[test]
fn reap_orphan_encoder() {
    let argv =
        |parts: &[&str]| parts.iter().map(std::string::ToString::to_string).collect::<Vec<_>>();
    let wallds = [4321];
    let orphan = argv(&[
        "ffmpeg",
        "-i",
        "/home/u/wallpaper-videos/long.mp4",
        "/home/u/.cache/skwd-wall-v2/video-opt/long-abc123.av1.mp4",
    ]);
    assert!(super::is_reapable_stale_renderer(&orphan, 1, &wallds, &[], &DIRS));
    assert!(!super::is_reapable_stale_renderer(&orphan, 4321, &wallds, &[], &DIRS));
    assert!(!super::is_reapable_stale_renderer(
        &argv(&["ffmpeg", "-i", "/home/u/movie.mkv", "/home/u/out.mp4"]),
        1,
        &wallds,
        &[],
        &DIRS
    ));
}

#[test]
fn empty_dirs_trap() {
    let argv =
        |parts: &[&str]| parts.iter().map(std::string::ToString::to_string).collect::<Vec<_>>();
    let wallds = [4321];
    let orphan = argv(&[
        "ffmpeg",
        "-i",
        "/home/u/wallpaper-videos/long.mp4",
        "/home/u/.cache/skwd-wall-v2/video-opt/long-abc123.av1.mp4",
    ]);

    assert!(super::is_reapable_stale_renderer(&argv(&["skwd-wall-still"]), 1, &wallds, &[], &[]));
    assert!(
        !super::is_reapable_stale_renderer(&orphan, 1, &wallds, &[], &[]),
        "empty dirs skip encoders"
    );
    assert!(super::is_reapable_stale_renderer(&orphan, 1, &wallds, &[], &DIRS));
}

#[test]
fn reap_ytdlp_ffmpeg() {
    let argv =
        |parts: &[&str]| parts.iter().map(std::string::ToString::to_string).collect::<Vec<_>>();
    let grandchild = argv(&[
        "ffmpeg",
        "-i",
        "https://rr3---sn-x.googlevideo.com/videoplayback?expire=1",
        "file:/home/u/wallpaper-videos/youtube-abc.mp4.part",
    ]);
    assert!(super::is_reapable_stale_renderer(&grandchild, 1, &[4321], &[], &DIRS));
    assert!(!super::is_reapable_stale_renderer(&grandchild, 4321, &[4321], &[], &DIRS));
    let elsewhere = argv(&["ffmpeg", "-i", "x.mp4", "file:/home/u/personal/clip.mp4.part"]);
    assert!(!super::is_reapable_stale_renderer(&elsewhere, 1, &[4321], &[], &DIRS));
}

#[test]
fn collect_orphans() {
    let root = tmp("proc");
    let mkproc = |pid: &str, cmdline: &[&str], ppid: i32| {
        let dir = root.join(pid);
        std::fs::create_dir_all(&dir).unwrap();
        let mut bytes = Vec::new();
        for arg in cmdline {
            bytes.extend_from_slice(arg.as_bytes());
            bytes.push(0);
        }
        std::fs::write(dir.join("cmdline"), bytes).unwrap();
        std::fs::write(dir.join("status"), format!("Name:\tx\nPPid:\t{ppid}\n")).unwrap();
    };
    mkproc("100", &["/usr/bin/skwd-wall-still", "--out", "DP-1"], 1);
    mkproc("200", &["/usr/bin/skwd-wall-still", "--out", "DP-1"], 4321);
    mkproc("300", &["/opt/linux-wallpaperengine", "42"], 1);
    mkproc("400", &["skwd-wall-scan", "--analyze"], 1);
    mkproc("500", &["skwd-wall-scan"], 1);
    mkproc("600", &["skwd-wall-vk", "*", "/v/a.mp4"], 1051);
    mkproc("700", &["skwd-wall-vk", "--video-stream", "/v/b.mp4"], 1051);
    mkproc("4321", &["/usr/bin/skwd-walld"], 1);
    mkproc("1051", &["/usr/bin/plasmashell"], 1);
    std::fs::create_dir_all(root.join("not-a-pid")).unwrap();

    let mut got = super::collect_reapable_renderers(&root, &DIRS);
    got.sort_unstable();
    assert_eq!(got, vec![100, 300, 400, 600]);
    let _ = std::fs::remove_dir_all(&root);
}
