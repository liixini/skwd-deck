#![cfg(test)]

use crate::testenv::tmp;
use serde_json::{Value, json};

#[test]
fn push_capped() {
    let mut paths: Vec<std::path::PathBuf> = Vec::new();
    assert!(super::push_capped(&mut paths, "/a".into(), 2));
    assert!(super::push_capped(&mut paths, "/b".into(), 2));
    assert!(!super::push_capped(&mut paths, "/c".into(), 2));
    assert!(!super::push_capped(&mut paths, "/a".into(), 2));
    assert_eq!(paths.len(), 2);
}

#[test]
fn hold_exceeded_bounds() {
    use std::time::Duration;
    assert!(!super::hold_exceeded(Duration::from_secs(9), Duration::from_secs(10)));
    assert!(super::hold_exceeded(Duration::from_secs(10), Duration::from_secs(10)));
    assert!(super::hold_exceeded(Duration::from_secs(11), Duration::from_secs(10)));
}

#[test]
fn watch_flush_split() {
    let dir = tmp("flush");
    let live = dir.join("live.png");
    std::fs::write(&live, b"x").unwrap();
    let gone = dir.join("gone.png");
    let already_removed = dir.join("already.png");

    let (changed, to_remove) = super::plan_watch_flush(
        vec![live.clone(), gone.clone()],
        vec![already_removed.clone(), gone.clone()],
    );
    assert_eq!(changed, vec![live.clone()]);
    let mut expect = vec![gone, already_removed];
    expect.sort();
    assert_eq!(to_remove, expect);

    let (_changed, none) = super::plan_watch_flush(Vec::new(), vec![live]);
    assert!(none.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn watch_status_detail() {
    let dir = tmp("watchstat");
    let path = dir.join("watch-status.json");
    let mut runtime = super::status::RuntimeStatus::default();
    let unavailable = runtime.unavailable("inotify limit reached");
    super::write_status(&path, &serde_json::to_value(unavailable).unwrap());
    let status: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(status["ok"], json!(false));
    assert_eq!(status["degraded"], json!(true));
    assert_eq!(status["mode"], json!("unavailable"));
    assert_eq!(status["detail"], json!("inotify limit reached"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn polling_status_is_explicit_and_bounded() {
    let roots = vec![super::polling::PollingRoot::new(
        "/mnt/library".into(),
        String::from("operation unsupported"),
    )];
    let mut runtime = super::status::RuntimeStatus::default();
    let status = runtime.polling(&roots, std::time::Duration::from_secs(45));
    assert!(status.ok);
    assert!(status.degraded);
    assert_eq!(status.mode, wall_proto::library_watch_mode::POLLING);
    assert_eq!(status.interval_seconds, Some(45));
    assert_eq!(status.entry_budget_per_root, Some(4096));
    assert_eq!(status.roots[0].path, "/mnt/library");
    assert_eq!(status.roots[0].native_error.as_deref(), Some("operation unsupported"));
    assert_eq!(status.roots[0].last_successful_convergence_unix_ms, None);
    assert_eq!(status.roots[0].pending_scans, 0);
}

struct InjectedRootWatcher {
    failed: std::collections::BTreeSet<std::path::PathBuf>,
    attempted: Vec<std::path::PathBuf>,
}

impl super::RootWatcher for InjectedRootWatcher {
    fn watch_root(&mut self, path: &std::path::Path) -> Result<(), String> {
        self.attempted.push(path.to_path_buf());
        if self.failed.contains(path) {
            Err(String::from("injected watch failure"))
        } else {
            Ok(())
        }
    }
}

#[test]
fn only_failed_roots_enter_polling() {
    let native = std::path::PathBuf::from("/mnt/native");
    let failed = std::path::PathBuf::from("/mnt/unwatchable");
    let mut watcher = InjectedRootWatcher {
        failed: std::collections::BTreeSet::from([failed.clone()]),
        attempted: Vec::new(),
    };
    let polling = super::watch_media_dirs(&mut watcher, &[native.clone(), failed.clone()]);
    assert_eq!(polling.len(), 1);
    assert_eq!(polling[0].path, failed);
    assert_eq!(watcher.attempted, vec![native, polling[0].path.clone()]);
}

#[test]
fn nested_media_roots_are_watched_once() {
    let (_guard, root) = crate::testenv::lock();
    let parent = root.join("walls");
    let child = parent.join("videos");
    std::fs::create_dir_all(&child).unwrap();
    crate::testenv::write_config(json!({
        "paths": {
            "wallpaper": child.to_string_lossy(),
            "videoWallpaper": parent.to_string_lossy(),
        }
    }));
    let state = std::sync::Arc::new(skwd_wall_core::WallState::open().unwrap());
    assert_eq!(super::media_roots(&state), vec![parent]);
}

#[test]
fn failed_root_recovers_to_native_watch() {
    let root = std::path::PathBuf::from("/mnt/unwatchable");
    let mut watcher = InjectedRootWatcher {
        failed: std::collections::BTreeSet::from([root.clone()]),
        attempted: Vec::new(),
    };
    let mut failed = super::watch_media_dirs(&mut watcher, std::slice::from_ref(&root));
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].path, root);
    watcher.failed.clear();
    assert_eq!(super::recover_roots(&mut watcher, &mut failed), vec![root]);
    assert!(failed.is_empty());
    assert_eq!(watcher.attempted.len(), 2);
}

#[test]
fn watch_status_uses_the_legacy_cache_layout() {
    let (_guard, root) = crate::testenv::lock();
    assert_eq!(super::watch_status_path(), root.join("cache/skwd-wall-v2/watch-status.json"));
}

#[test]
fn polling_status_reports_failed_sweeps() {
    let mut root = super::polling::PollingRoot::new(
        "/definitely/not/a/library".into(),
        String::from("operation unsupported"),
    );
    assert!(matches!(root.advance(1), super::polling::PollAdvance::Failed(_)));
    let mut runtime = super::status::RuntimeStatus::default();
    let status = runtime.polling(&[root], std::time::Duration::from_secs(60));
    assert!(!status.ok);
    assert!(status.detail.contains("could not be read"));
    assert!(status.roots[0].last_poll_error.is_some());
}

#[test]
fn correlated_poll_scan_records_true_convergence() {
    let path = std::path::PathBuf::from("/mnt/library");
    let roots =
        vec![super::polling::PollingRoot::new(path.clone(), String::from("operation unsupported"))];
    let mut runtime = super::status::RuntimeStatus::default();
    runtime.polling(&roots, std::time::Duration::from_secs(60));
    let pending = runtime.register_scan("watch-1", std::slice::from_ref(&path), &[]);
    assert_eq!(pending.roots[0].pending_scans, 1);
    assert!(pending.roots[0].last_scan_requested_unix_ms.is_some());
    assert_eq!(pending.roots[0].last_successful_convergence_unix_ms, None);
    assert!(runtime.complete_scan("other").is_none());
    let converged = runtime.complete_scan("watch-1").unwrap();
    assert_eq!(converged.mode, wall_proto::library_watch_mode::POLLING);
    assert_eq!(converged.roots[0].pending_scans, 0);
    assert!(converged.roots[0].last_successful_convergence_unix_ms.is_some());
    assert!(converged.last_successful_convergence_unix_ms.is_some());
}

#[test]
fn overlapping_poll_scans_converge_after_the_last_completion() {
    let path = std::path::PathBuf::from("/mnt/library");
    let roots =
        vec![super::polling::PollingRoot::new(path.clone(), String::from("operation unsupported"))];
    let mut runtime = super::status::RuntimeStatus::default();
    runtime.polling(&roots, std::time::Duration::from_secs(60));
    runtime.register_scan("watch-1", std::slice::from_ref(&path), &[]);
    let pending = runtime.register_scan("watch-2", std::slice::from_ref(&path), &[]);
    assert_eq!(pending.roots[0].pending_scans, 2);

    let incomplete = runtime.complete_scan("watch-1").unwrap();
    assert_eq!(incomplete.roots[0].pending_scans, 1);
    assert_eq!(incomplete.roots[0].last_successful_convergence_unix_ms, None);
    assert_eq!(incomplete.last_successful_convergence_unix_ms, None);

    let converged = runtime.complete_scan("watch-2").unwrap();
    assert_eq!(converged.roots[0].pending_scans, 0);
    assert!(converged.roots[0].last_successful_convergence_unix_ms.is_some());
    assert!(converged.last_successful_convergence_unix_ms.is_some());
}

#[test]
fn native_recovery_waits_for_the_correlated_handoff_scan() {
    let path = std::path::PathBuf::from("/mnt/library");
    let roots =
        vec![super::polling::PollingRoot::new(path.clone(), String::from("operation unsupported"))];
    let mut runtime = super::status::RuntimeStatus::default();
    runtime.polling(&roots, std::time::Duration::from_secs(60));
    runtime.polling(&[], std::time::Duration::from_secs(60));
    let recovering = runtime.register_scan(
        "watch-recovery",
        std::slice::from_ref(&path),
        std::slice::from_ref(&path),
    );
    assert_eq!(recovering.mode, wall_proto::library_watch_mode::RECOVERING);
    assert!(recovering.degraded);
    assert_eq!(recovering.roots[0].pending_scans, 1);
    let recovered = runtime.complete_scan("watch-recovery").unwrap();
    assert_eq!(recovered.mode, wall_proto::library_watch_mode::NATIVE);
    assert!(!recovered.degraded);
    assert!(recovered.last_successful_convergence_unix_ms.is_some());
    assert!(recovered.roots.is_empty());
}

#[test]
fn transient_artifacts() {
    for path in [
        "/vid/youtube-abc.mp4.part",
        "/vid/youtube-abc.mp4.part-Frag0",
        "/vid/youtube-abc.f399.mp4",
        "/vid/youtube-abc.f399-drc.webm",
        "/vid/youtube-abc.ytdl",
        "/wp/convert.tmp",
        "/wp/a.CRDOWNLOAD",
    ] {
        assert!(super::is_transient(std::path::Path::new(path)), "{path}");
    }
    for path in [
        "/vid/youtube-abc.mp4",
        "/wp/a.png",
        "/wp/wallpaper.final.jpg",
        "/wp/my.f4.wallpaper.jpg",
        "/wp/noext",
        "/wp/.f399.mp4",
    ] {
        assert!(!super::is_transient(std::path::Path::new(path)), "{path}");
    }
}

#[test]
fn config_events_partials() {
    let cfg = std::path::PathBuf::from("/home/u/.config/skwd-wall-v2/config.json");
    let mut out: Vec<std::path::PathBuf> = Vec::new();
    for _ in 0..50 {
        super::split_config_events(vec!["/vid/youtube-abc.mp4.part".into()], &cfg, &mut out);
    }
    assert!(out.is_empty());
    super::split_config_events(
        vec!["/wp/.skwd-wall-v2/trash/images/original.png".into()],
        &cfg,
        &mut out,
    );
    assert!(out.is_empty());
    super::split_config_events(vec!["/vid/youtube-abc.mp4".into()], &cfg, &mut out);
    assert_eq!(out, vec![std::path::PathBuf::from("/vid/youtube-abc.mp4")]);
}

#[test]
fn config_events_flag() {
    let cfg = std::path::PathBuf::from("/home/u/.config/skwd-wall-v2/config.json");
    let mut out: Vec<std::path::PathBuf> = Vec::new();
    let hit = super::split_config_events(
        vec![
            cfg.clone(),
            "/home/u/.config/skwd-wall-v2/config.json.corrupt".into(),
            "/wp/a.png".into(),
        ],
        &cfg,
        &mut out,
    );
    assert!(hit);
    assert_eq!(out, vec![std::path::PathBuf::from("/wp/a.png")]);
    assert!(!super::split_config_events(vec!["/wp/b.png".into()], &cfg, &mut out));
    assert_eq!(out.len(), 2);
}

#[tokio::test(start_paused = true)]
async fn debounce_flushes() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<notify::Event>();
    tx.send(notify::Event::new(notify::EventKind::Modify(notify::event::ModifyKind::Any))).unwrap();
    assert!(matches!(
        super::next_step(&mut rx, true, super::WATCH_DEBOUNCE).await,
        super::WatchStep::Event(_)
    ));
    assert!(matches!(
        super::next_step(&mut rx, false, super::WATCH_DEBOUNCE).await,
        super::WatchStep::Flush
    ));
}

#[tokio::test]
async fn debounce_smoke_real_time() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<notify::Event>();
    let debounce = std::time::Duration::from_millis(20);
    let t0 = std::time::Instant::now();
    assert!(matches!(super::next_step(&mut rx, false, debounce).await, super::WatchStep::Flush));
    assert!(t0.elapsed() >= debounce);
    drop(tx);
    assert!(matches!(super::next_step(&mut rx, true, debounce).await, super::WatchStep::Closed));
}

#[test]
fn theme_event_kinds() {
    use notify::event::{CreateKind, ModifyKind, RemoveKind};

    let path = skwd_wall_core::theme_provider::provider_path("end4").unwrap();
    let create =
        notify::Event::new(notify::EventKind::Create(CreateKind::File)).add_path(path.clone());
    assert_eq!(super::theme_event_provider(&create), Some("end4"));

    let modify =
        notify::Event::new(notify::EventKind::Modify(ModifyKind::Any)).add_path(path.clone());
    assert_eq!(super::theme_event_provider(&modify), Some("end4"));

    let remove = notify::Event::new(notify::EventKind::Remove(RemoveKind::File)).add_path(path);
    assert_eq!(super::theme_event_provider(&remove), None);

    let unrelated = notify::Event::new(notify::EventKind::Modify(ModifyKind::Any))
        .add_path(std::path::PathBuf::from("/tmp/colors.json"));
    assert_eq!(super::theme_event_provider(&unrelated), None);
}
