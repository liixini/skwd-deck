use super::*;

#[test]
fn wire_names_defaults() {
    let wire = serde_json::json!({
        "name": "DP-1", "width": 2560, "height": 1440,
        "target": "DP-1", "connected": true,
        "logical_width": 1440, "logical_height": 2560, "current": "/w/a.jpg",
        "type": "video", "path": "/w/v.mp4", "we_id": "", "mute": false, "volume": 40,
        "fill": "fit", "audioShared": true, "paused": true, "manual_paused": true
    });
    let output: OutputStatus = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(output.kind, "video");
    assert_eq!(output.volume, 40);
    assert_eq!(output.fill, "fit");
    assert!(output.audio_shared);
    assert!(output.paused);
    assert!(output.manual_paused);
    assert!(output.is_connected());
    assert_eq!(output.target(), "DP-1");
    assert_eq!(output.logical_size(), (1440, 2560));
    assert_eq!(serde_json::to_value(&output).unwrap(), wire);

    let sparse: OutputStatus = serde_json::from_value(serde_json::json!({"name": "X"})).unwrap();
    assert_eq!(sparse.volume, 100);
    assert!(!sparse.mute);
    assert!(!sparse.paused);
    assert!(!sparse.manual_paused);
    assert!(sparse.fill.is_empty());
    assert!(!sparse.audio_shared);
    assert!(sparse.is_connected());
    assert_eq!(sparse.target(), "X");

    let legacy: OutputStatus = serde_json::from_value(serde_json::json!({
        "name": "DP-2", "width": 1920, "height": 1080
    }))
    .unwrap();
    assert_eq!(legacy.logical_size(), (1920, 1080));
}

#[test]
fn outputs_from_envelope() {
    let result = serde_json::json!({"outputs": [{"name": "DP-1"}]});
    assert_eq!(outputs_from(&result).len(), 1);
    assert!(outputs_from(&serde_json::json!({})).is_empty());
}

#[test]
fn monitor_key_is_additive() {
    let output: OutputStatus = serde_json::from_value(serde_json::json!({
        "name": "DP-7", "monitor": "Dell Inc. DELL P2723DE 317ZNR3"
    }))
    .unwrap();
    assert_eq!(output.monitor, "Dell Inc. DELL P2723DE 317ZNR3");
    assert_eq!(serde_json::to_value(&output).unwrap()["monitor"], "Dell Inc. DELL P2723DE 317ZNR3");
    let legacy: OutputStatus = serde_json::from_value(serde_json::json!({"name": "DP-7"})).unwrap();
    assert!(legacy.monitor.is_empty());
    assert!(serde_json::to_value(&legacy).unwrap().get("monitor").is_none());
}
