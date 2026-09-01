#![cfg(test)]

use super::*;

#[test]
fn structured_status_round_trips() {
    let status = LibraryWatchStatus {
        ok: true,
        degraded: true,
        mode: mode::POLLING.to_string(),
        detail: String::from("bounded polling fallback is active"),
        interval_seconds: Some(45),
        entry_budget_per_root: Some(4096),
        last_successful_convergence_unix_ms: Some(1234),
        roots: vec![LibraryWatchRootStatus {
            path: String::from("/mnt/library"),
            mode: mode::POLLING.to_string(),
            native_error: Some(String::from("operation unsupported")),
            last_completed_sweep_unix_ms: Some(1200),
            last_scan_requested_unix_ms: Some(1220),
            last_successful_convergence_unix_ms: Some(1234),
            pending_scans: 0,
            last_poll_error: None,
        }],
    };
    let encoded = serde_json::to_value(&status).unwrap();
    assert_eq!(encoded["mode"], mode::POLLING);
    assert_eq!(encoded["roots"][0]["pending_scans"], 0);
    assert_eq!(serde_json::from_value::<LibraryWatchStatus>(encoded).unwrap(), status);
}

#[test]
fn additive_fields_default_for_older_payloads() {
    let status: LibraryWatchStatus =
        serde_json::from_value(serde_json::json!({"ok": true, "mode": "native"})).unwrap();
    assert_eq!(status.mode, mode::NATIVE);
    assert!(status.roots.is_empty());
    assert_eq!(status.last_successful_convergence_unix_ms, None);
}
