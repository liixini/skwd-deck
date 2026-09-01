#![cfg(test)]
use skwd_wall_core::lock;

use super::*;

#[test]
fn counters_track() {
    let stats = Stats::new();
    stats.rpc("wall.list");
    stats.rpc("wall.apply");
    stats.rpc("wall.list");
    stats.event("skwd.wall.applied");
    stats.applied("video", "/x/v.mp4");
    stats.thumb();
    stats.error();
    let counters = stats.counters_json();
    assert_eq!(counters["rpc"], 3);
    assert_eq!(counters["events"], 1);
    assert_eq!(counters["applies"], 1);
    assert_eq!(counters["thumbs"], 1);
    assert_eq!(counters["errors"], 1);
    let banner = stats.banner(42, 8, 2, 150, 1, 30, 220, 2);
    assert!(banner.contains("wall.list 2"));
    assert!(banner.contains("video: /x/v.mp4"));
    assert!(banner.contains("walld 42 MB"));
    assert!(banner.contains("wallpaper 2 = 8 MB"));
    assert!(banner.contains("transitions 1 = 150 MB"));
    assert!(banner.contains("scanner 30 MB"));
    assert!(banner.contains("total 230 MB"));
    assert!(banner.contains("gpu vram   : renderers 220 MB"));
    assert!(banner.contains("skwd.wall.applied"));
}

#[test]
fn recent_ring_cap() {
    let stats = Stats::new();
    for idx in 0..40 {
        stats.rpc(&format!("m{idx}"));
    }
    let len = lock(&stats.recent).len();
    assert_eq!(len, RECENT_CAP);
}
