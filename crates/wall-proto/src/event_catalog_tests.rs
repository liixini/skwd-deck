use super::ev::*;

#[test]
fn preview_ready_round_trips() {
    let wire = serde_json::json!({"id": "wh:abc", "path": "/cache/previews/wh-abc.jpg"});
    let event: PreviewReady = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(event.id, "wh:abc");
    assert_eq!(serde_json::to_value(&event).unwrap(), wire);
}

#[test]
fn remote_thumb_round_trips() {
    let wire = serde_json::json!({"id": "12345", "path": "/cache/remote/steam/12345.webp"});
    let event: RemoteThumb = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(&event).unwrap(), wire);

    let failed: RemoteThumb =
        serde_json::from_value(serde_json::json!({"id": "12345", "path": ""})).unwrap();
    assert!(failed.path.is_empty());
}

#[test]
fn removed_round_trips() {
    let wire = serde_json::json!({"key": "nature/a.png"});
    let event: Removed = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(event.key, "nature/a.png");
    assert_eq!(serde_json::to_value(&event).unwrap(), wire);
}

#[test]
fn file_removed_round_trips() {
    let wire = serde_json::json!({"name": "c.png"});
    let event: FileRemoved = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(event.name, "c.png");
    assert_eq!(serde_json::to_value(&event).unwrap(), wire);
}

#[test]
fn file_renamed_round_trips() {
    let wire = serde_json::json!({
        "old_name": "a.png", "new_name": "a.avif", "filesize": 812_331,
        "mtime": 1_754_000_000, "width": 3840, "height": 2160
    });
    let event: FileRenamed = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(event.new_name, "a.avif");
    assert_eq!(event.width, 3840);
    assert_eq!(serde_json::to_value(&event).unwrap(), wire);
}

#[test]
fn folder_removed_round_trips() {
    let wire = serde_json::json!({"names": ["city/a.png", "city/b.png"]});
    let event: FolderRemoved = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(event.names.len(), 2);
    assert_eq!(serde_json::to_value(&event).unwrap(), wire);
}

#[test]
fn unsubscribed_round_trips() {
    let wire = serde_json::json!({"id": "4242", "ok": false, "warn": true});
    let event: Unsubscribed = serde_json::from_value(wire.clone()).unwrap();
    assert!(event.warn);
    assert_eq!(serde_json::to_value(&event).unwrap(), wire);
}

#[test]
fn power_changed_round_trips() {
    let wire = serde_json::json!({"on_battery": true, "source": "upower"});
    let event: PowerChanged = serde_json::from_value(wire.clone()).unwrap();
    assert!(event.on_battery);
    assert_eq!(serde_json::to_value(&event).unwrap(), wire);
}

#[test]
fn outputs_changed_round_trips() {
    let wire = serde_json::json!({"outputs": ["DP-1", "HDMI-A-1"]});
    let event: OutputsChanged = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(event.outputs, ["DP-1", "HDMI-A-1"]);
    assert_eq!(serde_json::to_value(&event).unwrap(), wire);
}

#[test]
fn applied_round_trips() {
    let wire = serde_json::json!({
        "key": "a.png", "path": "/walls/a.png", "type": "static", "random": false
    });
    let event: Applied = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(event.kind, "static");
    assert_eq!(event.path.as_deref(), Some("/walls/a.png"));
    assert_eq!(serde_json::to_value(&event).unwrap(), wire);

    let we_wire = serde_json::json!({
        "key": "we:4242", "we_id": "4242", "type": "we", "random": true
    });
    let we_event: Applied = serde_json::from_value(we_wire.clone()).unwrap();
    assert_eq!(we_event.we_id.as_deref(), Some("4242"));
    assert!(we_event.path.is_none());
    assert_eq!(serde_json::to_value(&we_event).unwrap(), we_wire);
}

#[test]
fn apply_result_round_trips() {
    let wire = serde_json::json!({
        "request_id": 7, "ok": false, "output": "*",
        "error_kind": "decode_failed", "detail": "no decoder"
    });
    let event: ApplyResult = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(event.error_kind.as_deref(), Some("decode_failed"));
    assert_eq!(serde_json::to_value(&event).unwrap(), wire);

    let queued_wire =
        serde_json::json!({"request_id": 8, "ok": true, "output": "DP-1", "queued": true});
    let queued: ApplyResult = serde_json::from_value(queued_wire.clone()).unwrap();
    assert_eq!(queued.queued, Some(true));
    assert_eq!(serde_json::to_value(&queued).unwrap(), queued_wire);

    let sparse: ApplyResult = serde_json::from_value(serde_json::json!({})).unwrap();
    assert!(sparse.ok);
}

#[test]
fn theme_done_round_trips() {
    let wire = serde_json::json!({
        "source": "/walls/a.png", "ok": true, "backend": "matugen",
        "requested": "pywal", "external": true
    });
    let event: ThemeDone = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(event.backend, "matugen");
    assert_eq!(serde_json::to_value(&event).unwrap(), wire);

    let worker_wire = serde_json::json!({
        "source": "/walls/a.png", "ok": false, "backend": "matugen", "requested": "matugen"
    });
    let worker: ThemeDone = serde_json::from_value(worker_wire.clone()).unwrap();
    assert!(worker.external.is_none());
    assert_eq!(serde_json::to_value(&worker).unwrap(), worker_wire);

    let sparse: ThemeDone = serde_json::from_value(serde_json::json!({"source": "x"})).unwrap();
    assert!(sparse.ok);
    assert!(ThemeDone::default().ok);
}

#[test]
fn scan_done_round_trips() {
    let wire = serde_json::json!({
        "count": 12,
        "total": 3131,
        "disk_full": false,
        "request_id": "watch-7"
    });
    let event: ScanDone = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(event.total, Some(3131));
    assert_eq!(event.request_id.as_deref(), Some("watch-7"));
    assert_eq!(serde_json::to_value(&event).unwrap(), wire);

    let sparse: ScanDone = serde_json::from_value(serde_json::json!({"count": 5})).unwrap();
    assert_eq!(sparse.total, None);
    assert!(!sparse.disk_full);
    assert_eq!(sparse.request_id, None);
}

#[test]
fn semantic_index_ready_round_trips() {
    let wire = serde_json::json!({"items": 3131, "fingerprint": 18_446_744_073_709_551_615_u64});
    let event: SemanticIndexReady = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(event.items, 3131);
    assert_eq!(serde_json::to_value(&event).unwrap(), wire);
}

#[test]
fn config_changed_round_trips() {
    let wire = serde_json::json!({});
    let event: ConfigChanged = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(&event).unwrap(), wire);
}

#[test]
fn watch_error_round_trips() {
    let wire = serde_json::json!({"detail": "inotify limit reached"});
    let event: WatchError = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(event.detail, "inotify limit reached");
    assert_eq!(serde_json::to_value(&event).unwrap(), wire);
}

#[test]
fn recompute_progress_round_trips() {
    let wire = serde_json::json!({"progress": 40, "total": 100});
    let event: RecomputeProgress = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(event.progress, 40);
    assert_eq!(serde_json::to_value(&event).unwrap(), wire);
}

#[test]
fn recompute_complete_round_trips() {
    let wire = serde_json::json!({"updated": 98, "total": 100});
    let event: RecomputeComplete = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(event.updated, 98);
    assert_eq!(serde_json::to_value(&event).unwrap(), wire);
}

#[test]
fn image_optimize_complete_round_trips() {
    let wire = serde_json::json!({
        "running": false, "candidates": 20, "optimized": 15, "skipped_quality": 2,
        "skipped_savings": 2, "skipped_other": 1, "errors": 0, "original_bytes": 90_000_000_u64,
        "optimized_bytes": 30_000_000_u64, "trash_deleted_files": 3,
        "trash_deleted_bytes": 12_000_000_u64, "optimized_paths": ["/walls/a.avif"]
    });
    let event: ImageOptimizeComplete = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(event.optimized, 15);
    assert_eq!(event.optimized_paths, ["/walls/a.avif"]);
    assert_eq!(serde_json::to_value(&event).unwrap(), wire);
}
