use super::*;

#[test]
fn status_wire_shape() {
    let encoded = serde_json::to_value(Status::default()).unwrap();
    assert_eq!(encoded["pack"], "missing");
    assert_eq!(encoded["phase"], "idle");
    assert_eq!(encoded["prepared"], false);
    assert_eq!(encoded["enabled"], false);
    let decoded: Status = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded.phase, Phase::Idle);
}

#[test]
fn card_enable_wire() {
    let request = SetEnabledRequest { key: "static:one.jpg".into(), enabled: true };
    let encoded = serde_json::to_value(&request).unwrap();
    assert_eq!(encoded, serde_json::json!({"key":"static:one.jpg","enabled":true}));
    assert_eq!(serde_json::from_value::<SetEnabledRequest>(encoded).unwrap().key, request.key);
    let previous: Status = serde_json::from_value(serde_json::json!({
        "pack":"missing", "pack_path":"", "pack_bytes":0, "key":"", "active_key":"",
        "phase":"idle", "stage":"", "progress":0, "prepared":false,
        "preview_path":null, "cache_bytes":0, "elapsed_ms":0, "completed":0, "total":0,
        "error":null
    }))
    .unwrap();
    assert!(!previous.enabled);
    assert_eq!(previous.failed, 0);
}

#[test]
fn selected_key_request() {
    let request: KeyRequest = serde_json::from_str(r#"{"key":"static:one.jpg"}"#).unwrap();
    assert_eq!(request.key, "static:one.jpg");
    assert!(request.output.is_none());
}

#[test]
fn routes_drawing_methods() {
    let expected = [
        crate::rpc::DRAWING_STATUS,
        crate::rpc::DRAWING_REGISTER,
        crate::rpc::DRAWING_PREPARE,
        crate::rpc::DRAWING_CANCEL,
        crate::rpc::DRAWING_PAUSE,
        crate::rpc::DRAWING_RESUME,
        crate::rpc::DRAWING_DETACH,
        crate::rpc::DRAWING_CLEAR_CACHE,
        crate::rpc::DRAWING_PREVIEW,
        crate::rpc::DRAWING_SET_ENABLED,
    ];
    assert_eq!(crate::rpc::DRAWING_METHODS, expected.as_slice());
    assert!(!crate::rpc::DRAWING_METHODS.contains(&crate::rpc::STATUS));
}

#[test]
fn progress_line_shape() {
    let line = r#"{"stage":"lineart","progress":0.28}"#;
    let progress: Progress = serde_json::from_str(line).unwrap();
    assert_eq!(progress, Progress { stage: stage::LINEART.into(), progress: 0.28 });
    assert_eq!(serde_json::to_string(&progress).unwrap(), line);
}

#[test]
fn library_prepare_wire() {
    let request: PrepareRequest = serde_json::from_str(r#"{"library":true}"#).unwrap();
    assert!(request.library && request.keys.is_empty());
    assert!(!serde_json::from_str::<PrepareRequest>(r#"{"key":"static:a.png"}"#).unwrap().library);
}

#[test]
fn device_wire_labels() {
    let labels: Vec<&str> = DEVICE_NAMES.iter().map(|(device, _)| device.label()).collect();
    assert_eq!(labels, ["cpu", "cuda", "webgpu", "auto"]);
    for (device, name) in DEVICE_NAMES {
        assert_eq!(device.label(), name);
        let decoded: Device = serde_json::from_value(device.label().into()).unwrap();
        assert_eq!((decoded, Device::parse(device.label())), (device, Some(device)));
    }
    assert_eq!(Device::parse("rocm"), None);
    assert!(serde_json::from_value::<Device>("rocm".into()).is_err());
    assert_eq!(arg::DEVICE, "--device");
}

#[test]
fn pack_missing_after_check() {
    for (pack, phase, missing) in [
        (PackState::Missing, Phase::Idle, true),
        (PackState::Missing, Phase::Failed, true),
        (PackState::Missing, Phase::Checking, false),
        (PackState::Invalid, Phase::Idle, false),
        (PackState::Available, Phase::Ready, false),
    ] {
        let status = Status { pack, phase, ..Status::default() };
        assert_eq!(status.pack_missing(), missing, "{pack:?} {phase:?}");
    }
}
