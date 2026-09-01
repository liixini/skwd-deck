use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::super::artifacts::{
    clear_scan_error, image_needs_thumb, scan_error_fresh, write_scan_error,
};
use super::super::catalog::{Row, row_item_json};
use super::super::concurrency::{
    DECODE_MAX_CONCURRENT, DECODE_MEMORY_MIN_MIB, DecodeBudget, decode_max_concurrent,
    decode_memory_budget_bytes, scan_threads,
};
use super::common::is_image;
use super::delta::{changed_kind, needs_regen};

#[test]
fn image_extensions_are_case_insensitive() {
    assert!(is_image(Path::new("/x/a.PNG")));
    assert!(is_image(Path::new("/x/a.webp")));
    assert!(!is_image(Path::new("/x/a.mp4")));
    assert!(!is_image(Path::new("/x/a")));
}

#[test]
fn scan_thread_count_is_bounded() {
    let count = scan_threads();
    assert!((1..=8).contains(&count));
}

#[test]
fn changed_kind_routing() {
    let wallpapers = Path::new("/wp");
    let videos = Path::new("/vid");
    assert_eq!(
        changed_kind(Path::new("/wp/sub/a.png"), wallpapers, videos),
        Some(("static", "sub/a.png".to_string()))
    );
    assert_eq!(
        changed_kind(Path::new("/vid/clip.mp4"), wallpapers, videos),
        Some(("video", "clip.mp4".to_string()))
    );
    assert_eq!(
        changed_kind(Path::new("/wp/clip.mp4"), wallpapers, videos),
        Some(("video", "clip.mp4".to_string())),
    );
    assert_eq!(changed_kind(Path::new("/wp/notes.txt"), wallpapers, videos), None);
    assert_eq!(
        changed_kind(Path::new("/wp/.skwd-wall-v2/trash/images/a.png"), wallpapers, videos,),
        None
    );
    assert_eq!(changed_kind(Path::new("/elsewhere/a.png"), wallpapers, videos), None);
}

#[test]
fn regen_needs_artifact() {
    let mut known = HashMap::new();
    known.insert("static:a.png".to_string(), 100_i64);
    assert!(!needs_regen(&known, "static:a.png", 100, true));
    assert!(needs_regen(&known, "static:a.png", 200, true));
    assert!(needs_regen(&known, "static:a.png", 100, false));
    assert!(needs_regen(&known, "static:new.png", 100, true));
}

#[test]
fn decode_gate_bounds_concurrency() {
    let permits = 2_usize;
    let semaphore = Arc::new(DecodeBudget::new(16, permits));
    let live = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();
    for _ in 0..16 {
        let semaphore = semaphore.clone();
        let live = live.clone();
        let peak = peak.clone();
        handles.push(std::thread::spawn(move || {
            let _permit = semaphore.acquire(8);
            let current = live.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(current, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(5));
            live.fetch_sub(1, Ordering::SeqCst);
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
    assert!(peak.load(Ordering::SeqCst) <= permits);
    assert_eq!(live.load(Ordering::SeqCst), 0);
}

#[test]
fn decode_permit_count_is_bounded() {
    let count = decode_max_concurrent();
    assert!((1..=DECODE_MAX_CONCURRENT).contains(&count));
    assert!(decode_memory_budget_bytes() >= DECODE_MEMORY_MIN_MIB * 1024 * 1024);
}

#[test]
fn decode_gate_bytes() {
    let budget = Arc::new(DecodeBudget::new(10, 3));
    let first = budget.acquire(7);
    let entered = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let worker_budget = Arc::clone(&budget);
    let worker_entered = Arc::clone(&entered);
    let worker = std::thread::spawn(move || {
        let _permit = worker_budget.acquire(4);
        worker_entered.store(true, Ordering::Release);
    });
    std::thread::sleep(std::time::Duration::from_millis(10));
    assert!(!entered.load(Ordering::Acquire));
    drop(first);
    worker.join().unwrap();
    assert!(entered.load(Ordering::Acquire));
}

#[test]
fn exclusive_decode_drains() {
    let budget = Arc::new(DecodeBudget::new(10, 3));
    let first = budget.acquire(4);
    let entered = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let worker_budget = Arc::clone(&budget);
    let worker_entered = Arc::clone(&entered);
    let worker = std::thread::spawn(move || {
        let _permit = worker_budget.acquire_exclusive();
        worker_entered.store(true, Ordering::Release);
    });
    std::thread::sleep(std::time::Duration::from_millis(10));
    assert!(!entered.load(Ordering::Acquire));
    drop(first);
    worker.join().unwrap();
    assert!(entered.load(Ordering::Acquire));
}

#[test]
#[ignore = "manual scanner memory benchmark; set SKWD_BENCH_IMAGE"]
fn scanner_parallel_image_benchmark() {
    let source = std::env::var_os("SKWD_BENCH_IMAGE")
        .map(std::path::PathBuf::from)
        .expect("set SKWD_BENCH_IMAGE to a representative image");
    let directory = tempfile::tempdir().unwrap();
    let started = std::time::Instant::now();
    std::thread::scope(|scope| {
        for index in 0..3 {
            let source = &source;
            let root = directory.path();
            scope.spawn(move || {
                let _permit = super::super::concurrency::decode_budget()
                    .acquire(super::super::concurrency::image_decode_weight(source));
                crate::media::generate_image_thumbs(
                    source,
                    &root.join(format!("thumb-{index}.webp")),
                    &root.join(format!("small-{index}.webp")),
                )
                .unwrap();
            });
        }
    });
    eprintln!(
        "scanner-parallel-image-benchmark budget_mib={} elapsed_ms={}",
        super::super::concurrency::decode_memory_budget_bytes() / (1024 * 1024),
        started.elapsed().as_millis()
    );
}

#[test]
fn negative_cache_skips_unchanged_failure() {
    let directory = tempfile::tempdir().unwrap();
    let thumb = directory.path().join("thumbs/broken.webp");
    let known: HashMap<String, i64> = HashMap::new();
    let key = "static:broken.png";

    assert!(image_needs_thumb(&known, key, 100, false, false));

    write_scan_error(&thumb, 100);
    assert!(scan_error_fresh(&thumb, 100));
    assert!(!image_needs_thumb(&known, key, 100, false, scan_error_fresh(&thumb, 100)));
    assert!(image_needs_thumb(&known, key, 200, false, scan_error_fresh(&thumb, 200)));

    clear_scan_error(&thumb);
    assert!(!scan_error_fresh(&thumb, 100));
}

#[test]
fn negative_cache_requires_thumbnail_artifact() {
    let mut known = HashMap::new();
    known.insert("static:ok.png".to_string(), 100_i64);
    assert!(!image_needs_thumb(&known, "static:ok.png", 100, true, false));
    assert!(image_needs_thumb(&known, "static:ok.png", 100, false, false));
}

#[test]
fn row_json_wire_shape() {
    let row = Row {
        key: "static:a.png".into(),
        name: "a.png".into(),
        thumb: "/t.webp".into(),
        thumb_sm: "/s.webp".into(),
        mtime: 1,
        hue: 4,
        sat: 50,
        richness: 200,
        filesize: 9,
        width: 1920,
        height: 1080,
    };
    let json = row_item_json(&row);
    assert_eq!(json["key"], "static:a.png");
    assert_eq!(json["type"], "static");
    assert_eq!(json["width"], 1920);
    assert!(json.get("thumb_sm").is_some());
}
