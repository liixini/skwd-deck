use super::{event_hub, events, subscribe};
use serde_json::json;
use std::path::Path;

#[test]
fn preset_process_waits_for_parent_and_links_both_items() {
    let root = tempfile::tempdir().unwrap();
    let preset = root.path().join("preset/2");
    let parent = root.path().join("parent/1");
    std::fs::create_dir_all(&preset).unwrap();
    std::fs::create_dir_all(&parent).unwrap();
    std::fs::write(preset.join("project.json"), r#"{"dependency":"1","preset":{"rain":false}}"#)
        .unwrap();
    std::fs::write(parent.join("project.json"), r#"{"type":"scene"}"#).unwrap();
    std::fs::write(parent.join("scene.pkg"), b"fixture").unwrap();
    let hub = event_hub();
    let mut rx = subscribe(&hub);
    let library = root.path().join("library");
    let mut fetched = Vec::new();
    assert!(super::super::presets::download(
        hub.as_ref(),
        &library,
        &["2".into()],
        |progress, ids| {
            fetched.extend_from_slice(ids);
            let folder = if ids[0] == "2" { &preset } else { &parent };
            let command =
                helper(root.path(), &[json!({"id":ids[0],"status":"done","folder":folder})], 0);
            super::super::run_steamworks_command(progress, &library, ids, command)
        }
    ));
    assert_eq!(fetched, ["2", "1"]);
    assert_eq!(library.join("2").canonicalize().unwrap(), preset);
    assert_eq!(library.join("1").canonicalize().unwrap(), parent);
    let received = events(&mut rx);
    let completed: Vec<_> =
        received.iter().filter(|event| event.data["status"] == "done").collect();
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].data["id"], "2");
    assert_eq!(received.last().unwrap().data["status"], "done");
}

fn helper(root: &Path, records: &[serde_json::Value], exit_code: u8) -> std::process::Command {
    let script = root.join("helper.sh");
    let output = records.iter().map(serde_json::Value::to_string).collect::<Vec<_>>().join("\n");
    std::fs::write(
        &script,
        format!(
            "printf '%s\\n' \"$@\" > '{}'\ncat <<'RECORDS'\nSteam diagnostic noise\n{output}\nRECORDS\nexit {exit_code}\n",
            root.join("arguments").display()
        ),
    )
    .unwrap();
    let mut command = crate::infrastructure::proc::tool("/bin/sh");
    command.arg(script);
    command
}

#[test]
fn installed_item_is_linked_before_completion_and_receives_requested_id() {
    let root = tempfile::tempdir().unwrap();
    let actual = root.path().join("steam/431960/12345");
    std::fs::create_dir_all(&actual).unwrap();
    std::fs::write(actual.join("project.json"), "{}").unwrap();
    let command = helper(
        root.path(),
        &[
            json!({"id":"12345", "status":"downloading", "progress":0.5}),
            json!({"id":"12345", "status":"done", "folder":actual}),
        ],
        0,
    );
    let hub = event_hub();
    let mut rx = subscribe(&hub);
    let library = root.path().join("library");
    assert!(super::super::run_steamworks_command(
        hub.as_ref(),
        &library,
        &["12345".into()],
        command
    ));
    assert_eq!(std::fs::read_link(library.join("12345")).unwrap(), actual);
    assert!(library.join("12345/project.json").is_file());
    assert_eq!(std::fs::read_to_string(root.path().join("arguments")).unwrap(), "12345\n");
    let received = events(&mut rx);
    assert_eq!(received.len(), 3);
    assert_eq!(received[0].data["progress"], json!(0.5));
    assert_eq!(received[1].data["status"], "downloading");
    assert_eq!(received[2].data["status"], "done");
}

#[test]
fn helper_done_without_an_installed_folder_is_an_error() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("missing");
    let command =
        helper(root.path(), &[json!({"id":"12345", "status":"done", "folder":missing})], 0);
    let hub = event_hub();
    let mut rx = subscribe(&hub);
    assert!(!super::super::run_steamworks_command(
        hub.as_ref(),
        &root.path().join("library"),
        &["12345".into()],
        command
    ));
    let received = events(&mut rx);
    assert!(received.iter().all(|event| event.data["status"] != "done"));
    assert_eq!(received.last().unwrap().data["status"], "error");
}

#[test]
fn steam_login_error_reaches_the_download_event() {
    let root = tempfile::tempdir().unwrap();
    let message = "Steam is not running, or the account does not own Wallpaper Engine";
    let command =
        helper(root.path(), &[json!({"id":"12345", "status":"error", "message":message})], 1);
    let hub = event_hub();
    let mut rx = subscribe(&hub);
    assert!(!super::super::run_steamworks_command(
        hub.as_ref(),
        &root.path().join("library"),
        &["12345".into()],
        command
    ));
    let received = events(&mut rx);
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].data["status"], "error");
    assert_eq!(received[0].data["message"], message);
}

#[test]
fn partial_batch_only_completes_the_item_that_landed() {
    let root = tempfile::tempdir().unwrap();
    let actual = root.path().join("installed");
    std::fs::create_dir_all(&actual).unwrap();
    let command = helper(
        root.path(),
        &[
            json!({"id":"12345", "status":"done", "folder":actual}),
            json!({"id":"67890", "status":"error", "message":"Download failed"}),
        ],
        1,
    );
    let hub = event_hub();
    let mut rx = subscribe(&hub);
    assert!(super::super::run_steamworks_command(
        hub.as_ref(),
        &root.path().join("library"),
        &["12345".into(), "67890".into()],
        command
    ));
    let received = events(&mut rx);
    let completed: Vec<_> =
        received.iter().filter(|event| event.data["status"] == "done").collect();
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].data["id"], "12345");
    let failed = received.last().unwrap();
    assert_eq!(failed.data["id"], "67890");
    assert_eq!(failed.data["status"], "error");
    assert_eq!(failed.data["message"], "Download failed");
}

#[test]
fn missing_helper_reports_the_optional_package_for_every_item() {
    let root = tempfile::tempdir().unwrap();
    let command = crate::infrastructure::proc::tool(root.path().join("missing-helper"));
    let hub = event_hub();
    let mut rx = subscribe(&hub);
    assert!(!super::super::run_steamworks_command(
        hub.as_ref(),
        root.path(),
        &["12345".into(), "67890".into()],
        command
    ));
    let received = events(&mut rx);
    assert_eq!(received.len(), 2);
    for (event, id) in received.iter().zip(["12345", "67890"]) {
        assert_eq!(event.data["id"], id);
        assert_eq!(event.data["status"], "error");
        assert!(event.data["message"].as_str().unwrap().contains("skwd-deck-steamworks"));
    }
}
