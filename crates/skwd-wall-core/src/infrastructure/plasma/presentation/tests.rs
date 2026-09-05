use super::*;

#[test]
fn every_output_must_accept_a_frame_and_requests_are_removed() {
    let dir = tempfile::tempdir().unwrap();
    let mut payload = serde_json::json!({"DP-1":{},"DP-2":{}});
    let pending = Pending::create(dir.path(), &mut payload).unwrap();
    assert!(pending.wait(Duration::ZERO).unwrap_err().to_string().contains("DP-1"));
    std::fs::write(&pending.files[0].1, br#"{"state":"ready"}"#).unwrap();
    assert!(pending.wait(Duration::ZERO).unwrap_err().to_string().contains("DP-2"));
    std::fs::write(&pending.files[1].1, br#"{"state":"error","error":"export unavailable"}"#)
        .unwrap();
    assert!(pending.wait(Duration::ZERO).unwrap_err().to_string().contains("export unavailable"));
    std::fs::write(&pending.files[1].1, br#"{"state":"ready"}"#).unwrap();
    pending.wait(Duration::ZERO).unwrap();
    let paths = pending.files.clone();
    drop(pending);
    assert!(paths.iter().all(|(_, path)| !path.exists()));
}
