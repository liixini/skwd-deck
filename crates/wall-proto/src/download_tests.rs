use super::*;

#[test]
fn event_omits_absent_fields() {
    let wire = serde_json::json!({
        "id": "wh:abc", "status": "downloading", "progress": 0.5, "message": "fetching"
    });
    let event: DownloadEvent = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(event.progress, Some(0.5));
    assert_eq!(serde_json::to_value(&event).unwrap(), wire);

    let done =
        DownloadEvent { path: Some("/w/a.jpg".into()), ..DownloadEvent::new("wh:abc", "done") };
    assert_eq!(
        done.to_value(),
        serde_json::json!({"id": "wh:abc", "status": "done", "path": "/w/a.jpg"})
    );

    let error: DownloadEvent =
        serde_json::from_value(serde_json::json!({"id": "x", "status": "error", "error": "boom"}))
            .unwrap();
    assert_eq!(error.error.as_deref(), Some("boom"));
    assert_eq!(error.progress, None);
}
