use super::*;
use std::os::unix::fs::PermissionsExt;

fn prepared_session(name: &str) -> (Sandbox, FakePlasma) {
    let (mut sandbox, plasma) = plasma_session(name);
    let mut config: Value =
        serde_json::from_slice(&std::fs::read(sandbox.config_path()).unwrap()).unwrap();
    config["paper"] = json!({"videoEngine":"tinier"});
    sandbox.write_config(&config);
    let shim = sandbox.root.join("plasma/bin/ffmpeg");
    std::fs::write(
        &shim,
        r#"#!/bin/sh
case "$*" in
  *libsvtav1*)
    touch "$SKWD_E2E_ENCODE_GATE.started"
    while ! test -e "$SKWD_E2E_ENCODE_GATE"; do echo out_time_us=0; sleep 0.01; done
    ;;
esac
exec /usr/bin/ffmpeg "$@"
"#,
    )
    .unwrap();
    std::fs::set_permissions(shim, std::fs::Permissions::from_mode(0o755)).unwrap();
    sandbox.set_env("SKWD_E2E_ENCODE_GATE", &sandbox.root.join("encode").to_string_lossy());
    (sandbox, plasma)
}

fn video(sandbox: &Sandbox, name: &str, color: &str) -> String {
    let path = sandbox.library().join(name);
    assert!(skwd_e2e::ffmpeg_video(&path, color, 1.0));
    path.to_string_lossy().into_owned()
}

fn queued(walld: &Walld, client: &mut Client, path: &str, output: &str, token: &str) -> Value {
    let response = call(
        walld,
        client,
        "wall.apply",
        json!({"type":"video","path":path,"output":output,"activity":"activity-a","apply_token":token}),
    );
    assert_eq!(response["result"]["queued"], true, "{response}");
    assert_eq!(response["result"]["visible"], false, "{response}");
    assert_eq!(response["result"]["activity"], "activity-a");
    assert_eq!(response["result"]["apply_token"], token);
    response["result"].clone()
}

fn original_path(sandbox: &Sandbox, output: &str) -> String {
    let connection = rusqlite::Connection::open(sandbox.sqlite_path()).unwrap();
    let state = sandbox.outputs_json();
    skwd_wall_core::db::tinier_convert_src(&connection, state[output]["path"].as_str().unwrap())
        .unwrap()
        .unwrap()
}

fn terminals(subscriber: &mut Client, count: usize) -> Vec<Value> {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    let mut results = Vec::new();
    while results.len() < count && std::time::Instant::now() < deadline {
        if let Some(event) = subscriber.recv(Duration::from_millis(100))
            && event["event"] == wall_proto::ev::APPLY_RESULT
            && event["data"]["queued"] != true
        {
            results.push(event["data"].clone());
        }
    }
    assert_eq!(results.len(), count, "{results:?}");
    results
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn cold_videos_preserve_targets() {
    let (sandbox, _plasma) = prepared_session("activity-cold-targets");
    let left = video(&sandbox, "a.mp4", "blue");
    let right = video(&sandbox, "b.mp4", "yellow");
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let mut subscriber = walld.client();
    call(&walld, &mut subscriber, "subscribe", json!({}));
    queued(&walld, &mut client, &left, "DP-1", "left");
    assert!(wait_until(|| sandbox.root.join("encode.started").exists(), Duration::from_secs(5)));
    queued(&walld, &mut client, &right, "DP-2", "right");
    std::fs::write(sandbox.root.join("encode"), b"").unwrap();
    let results = terminals(&mut subscriber, 2);
    for (token, output, path) in [("left", "DP-1", left), ("right", "DP-2", right)] {
        let result = results.iter().find(|result| result["apply_token"] == token).unwrap();
        assert_eq!(result["ok"], true, "{result}: {}", walld.log_contents());
        assert_eq!(result["visible"], true);
        assert_eq!(result["output"], output);
        assert_eq!(result["activity"], "activity-a");
        assert!(result["task_id"].as_str().is_some());
        assert_eq!(original_path(&sandbox, output), path);
    }
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn latest_target_supersedes_preparation() {
    let (sandbox, _plasma) = prepared_session("activity-cold-replace");
    let first = video(&sandbox, "a.mp4", "blue");
    let second = video(&sandbox, "b.mp4", "yellow");
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let mut subscriber = walld.client();
    call(&walld, &mut subscriber, "subscribe", json!({}));
    queued(&walld, &mut client, &first, "DP-1", "old");
    assert!(wait_until(|| sandbox.root.join("encode.started").exists(), Duration::from_secs(5)));
    queued(&walld, &mut client, &second, "DP-1", "new");
    std::fs::write(sandbox.root.join("encode"), b"").unwrap();
    let results = terminals(&mut subscriber, 2);
    let old = results.iter().find(|result| result["apply_token"] == "old").unwrap();
    assert_eq!(old["superseded"], true);
    assert_eq!(old["ok"], false);
    assert_eq!(old["visible"], false);
    assert!(results.iter().any(|result| result["apply_token"] == "new" && result["ok"] == true));
    assert_eq!(original_path(&sandbox, "DP-1"), second);
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn failed_preparation_reports_target() {
    let (sandbox, _plasma) = prepared_session("activity-cold-failure");
    let broken = sandbox.library().join("broken.mp4");
    std::fs::write(&broken, b"invalid video").unwrap();
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let mut subscriber = walld.client();
    call(&walld, &mut subscriber, "subscribe", json!({}));
    queued(&walld, &mut client, broken.to_str().unwrap(), "DP-1", "broken");
    let results = terminals(&mut subscriber, 1);
    assert_eq!(results[0]["apply_token"], "broken");
    assert_eq!(results[0]["ok"], false);
    assert_eq!(results[0]["visible"], false);
    assert_eq!(results[0]["activity"], "activity-a");
    assert_eq!(results[0]["output"], "DP-1");
    assert!(results[0]["detail"].as_str().is_some_and(|detail| !detail.is_empty()));
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn cancelled_preparation_finishes_request() {
    let (sandbox, _plasma) = prepared_session("activity-cold-cancel");
    let source = video(&sandbox, "a.mp4", "blue");
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let mut subscriber = walld.client();
    call(&walld, &mut subscriber, "subscribe", json!({}));
    let response = queued(&walld, &mut client, &source, "DP-1", "cancel");
    assert!(wait_until(|| sandbox.root.join("encode.started").exists(), Duration::from_secs(5)));
    call(
        &walld,
        &mut client,
        wall_proto::rpc::TASK_CONTROL,
        json!({"id":response["task_id"],"action":"stop"}),
    );
    let results = terminals(&mut subscriber, 1);
    assert_eq!(results[0]["apply_token"], "cancel");
    assert_eq!(results[0]["cancelled"], true);
    assert_eq!(results[0]["ok"], false);
    assert_eq!(results[0]["visible"], false);
    assert!(sandbox.outputs_json().get("DP-1").is_none());
}

fn switch_activity(sandbox: &Sandbox, current: &str) {
    std::fs::write(
        sandbox.root.join("plasma/activities.json"),
        json!({"current":current,"names":{"activity-a":"Work","activity-b":"Home"}}).to_string(),
    )
    .unwrap();
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn inactive_save_supersedes_preparation() {
    let (sandbox, _plasma) = prepared_session("activity-inactive-supersedes");
    let left = video(&sandbox, "a.mp4", "blue");
    let right = video(&sandbox, "b.mp4", "yellow");
    let saved = still(&sandbox, "saved.png", "green");
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let mut subscriber = walld.client();
    call(&walld, &mut subscriber, "subscribe", json!({}));
    queued(&walld, &mut client, &left, "DP-1", "old");
    assert!(wait_until(|| sandbox.root.join("encode.started").exists(), Duration::from_secs(5)));
    queued(&walld, &mut client, &right, "DP-2", "other");
    switch_activity(&sandbox, "activity-b");
    let response = call(
        &walld,
        &mut client,
        "wall.apply",
        json!({"type":"static","path":saved,"activity":"activity-a","output":"DP-1","apply_token":"saved"}),
    );
    assert_eq!(response["result"]["visible"], false);
    switch_activity(&sandbox, "activity-a");
    std::fs::write(sandbox.root.join("encode"), b"").unwrap();
    let results = terminals(&mut subscriber, 3);
    let old = results.iter().find(|result| result["apply_token"] == "old").unwrap();
    assert_eq!(old["superseded"], true, "{results:?}");
    assert_eq!(old["ok"], false);
    assert!(results.iter().any(|result| result["apply_token"] == "other" && result["ok"] == true));
    assert_eq!(sandbox.outputs_json()["DP-1"]["path"], saved);
    assert_eq!(original_path(&sandbox, "DP-2"), right);
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn failed_save_preserves_preparation() {
    let (sandbox, _plasma) = prepared_session("activity-inactive-failure");
    let source = video(&sandbox, "a.mp4", "blue");
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let mut subscriber = walld.client();
    call(&walld, &mut subscriber, "subscribe", json!({}));
    queued(&walld, &mut client, &source, "DP-1", "prepared");
    assert!(wait_until(|| sandbox.root.join("encode.started").exists(), Duration::from_secs(5)));
    switch_activity(&sandbox, "activity-b");
    let failed = client.call("wall.apply", json!({"type":"static","path":sandbox.library().join("missing.png"),"activity":"activity-a","output":"DP-1","apply_token":"failed"}), 2).unwrap();
    assert!(failed.get("error").is_some());
    switch_activity(&sandbox, "activity-a");
    std::fs::write(sandbox.root.join("encode"), b"").unwrap();
    let results = terminals(&mut subscriber, 2);
    let prepared = results.iter().find(|result| result["apply_token"] == "prepared").unwrap();
    assert_eq!(prepared["ok"], true, "{results:?}");
    assert_eq!(original_path(&sandbox, "DP-1"), source);
}
