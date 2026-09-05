#![cfg(test)]

use std::io::Write;
use std::path::PathBuf;

use serde_json::{Value, json};

use super::args::{first_positional, take_flag, take_option as take_opt};
use super::commands::run as run_verb;
use super::entry::VERBS;
use super::error::CliError as CliErr;
use super::look::{asset_present, sanitize_name};
use super::rpc::call;
use super::wallpaper::{
    apply_params_for_item, kind_for_path, matches_filters, now_seed, path_for_key, pick_index,
};
use super::watch::{expand_template, run as watch, shell_escape};

#[test]
fn key_to_path() {
    assert_eq!(path_for_key("static:a/b.png", "/w", "/v"), Some("/w/a/b.png".into()));
    assert_eq!(path_for_key("video:clip.mp4", "/w", "/v/"), Some("/v/clip.mp4".into()));
    assert_eq!(path_for_key("we:123", "/w", "/v"), None);
}

#[test]
fn kind_for_path_video() {
    assert_eq!(kind_for_path("/x/a.MP4"), "video");
    assert_eq!(kind_for_path("/x/a.webm"), "video");
    assert_eq!(kind_for_path("/x/a.png"), "static");
    assert_eq!(kind_for_path("/x/noext"), "static");
}

#[test]
fn apply_type_override() {
    let dir = std::env::temp_dir().join(format!("skwd-cli-type-ov-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("thing.png");
    std::fs::write(&file, b"x").unwrap();
    let daemon = fake_daemon(&[&[r#"{"id":1,"result":{}}"#]]);
    let result = run_verb(
        &daemon.sock,
        "apply",
        vec!["--type".into(), "video".into(), file.to_string_lossy().into_owned()],
    );
    assert_eq!(err_code(result), 0);
    let req = daemon.reqs.recv().expect("apply sent");
    assert_eq!(req["params"]["type"], "video");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn apply_params_per_type() {
    let item = |value: serde_json::Value| -> wall_proto::WallpaperItem {
        serde_json::from_value(value).unwrap()
    };
    let we = item(json!({ "type": "we", "we_id": "555", "key": "we:555" }));
    assert_eq!(
        apply_params_for_item(&we, "/w", "/v", "*"),
        Some(json!({ "type": "we", "we_id": "555", "output": "*" }))
    );
    let img = item(json!({ "type": "static", "key": "static:p.png" }));
    assert_eq!(
        apply_params_for_item(&img, "/w", "/v", "DP-1"),
        Some(json!({ "type": "static", "path": "/w/p.png", "output": "DP-1" }))
    );
    let vid = item(json!({ "type": "video", "key": "video:c.mp4" }));
    assert_eq!(
        apply_params_for_item(&vid, "/w", "/v", "*"),
        Some(json!({ "type": "video", "path": "/v/c.mp4", "output": "*" }))
    );
    let bad = item(json!({ "type": "we", "key": "we:1" }));
    assert_eq!(apply_params_for_item(&bad, "/w", "/v", "*"), None);
}

#[test]
fn pick_index_bounds() {
    assert_eq!(pick_index(0, 123), None);
    assert_eq!(pick_index(5, 12), Some(2));
    assert_eq!(pick_index(5, 5), Some(0));
    assert!(pick_index(3, now_seed()).unwrap() < 3);
}

#[test]
fn expand_template_tokens() {
    let data = json!({ "path": "/w/a.png", "type": "static", "output": "DP-1" });
    assert_eq!(
        expand_template("notify %event% %type% %path% on %output%", "skwd.wall.applied", &data),
        "notify 'skwd.wall.applied' 'static' '/w/a.png' on 'DP-1'"
    );
    assert_eq!(expand_template("echo %name%", "x", &json!({})), "echo ''");
    assert_eq!(expand_template("100% done %notatoken%", "x", &json!({})), "100% done %notatoken%");
}

#[test]
fn expand_template_metachars() {
    let data = json!({ "name": "\"; touch /tmp/pwned; \"" });
    assert_eq!(expand_template("notify %name%", "x", &data), "notify '\"; touch /tmp/pwned; \"'");
    let quoted = json!({ "name": "it's $(rm -rf ~) `bad`" });
    assert_eq!(expand_template("n %name%", "x", &quoted), "n 'it'\\''s $(rm -rf ~) `bad`'");
}

#[test]
fn expand_template_no_reexpand() {
    let data = json!({ "path": "%name%", "name": "INJECTED" });
    assert_eq!(expand_template("run %path%", "x", &data), "run '%name%'");
}

#[test]
fn shell_escape_roundtrip() {
    let hostile = "a'b\"; touch /tmp/pwned; $(id) `id` \\ $HOME";
    let out = std::process::Command::new("sh")
        .args(["-c", &format!("printf %s {}", shell_escape(hostile))])
        .output()
        .expect("sh runs");
    assert_eq!(String::from_utf8_lossy(&out.stdout), hostile);
}

#[test]
fn take_opt_flag() {
    let mut args = vec!["--output".into(), "DP-1".into(), "--json".into(), "x".into()];
    assert_eq!(take_opt(&mut args, &["--output", "-o"]), Some("DP-1".into()));
    assert!(take_flag(&mut args, &["--json"]));
    assert_eq!(first_positional(&args), Some("x".into()));
    assert!(!take_flag(&mut args, &["--missing"]));
}

#[test]
fn matches_filters_tags() {
    let item = |value: serde_json::Value| -> wall_proto::WallpaperItem {
        serde_json::from_value(value).unwrap()
    };
    let video = item(json!({ "type": "video", "tags": "nature, dark forest,calm" }));
    assert!(matches_filters(&video, None, None));
    assert!(matches_filters(&video, Some("video"), None));
    assert!(!matches_filters(&video, Some("static"), None));
    assert!(matches_filters(&video, None, Some("nature")));
    assert!(matches_filters(&video, None, Some("calm")));
    assert!(matches_filters(&video, None, Some("dark")));
    assert!(!matches_filters(&video, None, Some("dark forest")));
    assert!(!matches_filters(&video, None, Some("missing")));
    assert!(!matches_filters(&item(json!({ "type": "static" })), None, Some("any")));
}

#[test]
fn verb_table_coverage() {
    for typo in ["aply", "ls", "--output", "wallpaper.png", ""] {
        assert!(!VERBS.contains(&typo));
    }
    let sock = std::env::temp_dir().join(format!("skwd-cli-noverb-{}.sock", std::process::id()));
    let tmp = std::env::temp_dir().join(format!("skwd-cli-verbcover-{}", std::process::id()));
    for verb in VERBS {
        let args = match *verb {
            "export" => {
                vec!["--out".into(), tmp.join("pack").to_string_lossy().into_owned()]
            }
            "import" => {
                vec!["--dry-run".into(), tmp.join("nope").to_string_lossy().into_owned()]
            }
            _ => vec![],
        };
        if let Err(err) = run_verb(&sock, verb, args) {
            assert!(!err.message().contains("unknown verb"), "{verb} missing arm");
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn sanitize_name_stems() {
    assert_eq!(sanitize_name("My Look"), "My-Look");
    assert_eq!(sanitize_name("nord/../etc"), "nord----etc");
    assert_eq!(sanitize_name("   "), "look");
    assert_eq!(sanitize_name("ok_name-1"), "ok_name-1");
}

#[test]
fn asset_present_local() {
    let dir = std::env::temp_dir().join(format!("skwd-asset-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.png"), b"x").unwrap();
    let cfg = crate::config::Config::from_data(serde_json::json!({
        "paths": {"wallpaper": dir.to_string_lossy()}
    }));
    assert!(asset_present(&cfg, "static:a.png"));
    assert!(!asset_present(&cfg, "static:missing.png"));
    assert!(!asset_present(&cfg, "we:123"));
    let _ = std::fs::remove_dir_all(&dir);
}

use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

struct FakeDaemon {
    sock: PathBuf,
    reqs: mpsc::Receiver<Value>,
}

fn fake_daemon(scripts: &[&[&str]]) -> FakeDaemon {
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let sock = std::env::temp_dir().join(format!(
        "skwd-cli-test-{}-{}.sock",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_file(&sock);
    let listener = UnixListener::bind(&sock).expect("bind fake daemon socket");
    let scripts: Vec<Vec<String>> = scripts
        .iter()
        .map(|lines| lines.iter().map(std::string::ToString::to_string).collect())
        .collect();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for script in scripts {
            let Ok((stream, _)) = listener.accept() else { return };
            let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                return;
            }
            let _ = tx.send(serde_json::from_str(line.trim()).unwrap_or(Value::Null));
            let mut writer = stream;
            for reply in script {
                let _ = writer.write_all(reply.as_bytes());
                let _ = writer.write_all(b"\n");
            }
            let _ = writer.flush();
        }
    });
    FakeDaemon { sock, reqs: rx }
}

fn err_code(result: Result<(), CliErr>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(err) => err.code(),
    }
}

#[test]
fn call_skips_noise() {
    let daemon = fake_daemon(&[&[
        r#"{"event":"skwd.wall.applied","data":{"path":"/x"}}"#,
        "",
        "not json",
        r#"{"id":7,"result":{"stale":true}}"#,
        r#"{"id":1,"result":{"ok":true}}"#,
    ]]);
    let res = call(&daemon.sock, "wall.ping", &json!({}));
    assert_eq!(res.ok(), Some(json!({"ok": true})));
    let req = daemon.reqs.recv().expect("request reached daemon");
    assert_eq!(req["method"], "wall.ping");
    assert_eq!(req["id"], 1);
}

#[test]
fn call_rpc_error() {
    let daemon = fake_daemon(&[&[r#"{"id":1,"error":{"code":-1,"message":"boom"}}"#]]);
    let res = call(&daemon.sock, "wall.ping", &json!({}));
    match res {
        Err(CliErr::Rpc(code, message)) => {
            assert_eq!(code, -1);
            assert_eq!(message, "boom");
            assert_eq!(CliErr::Rpc(code, message).code(), 1);
        }
        _ => panic!("expected CliErr::Rpc"),
    }
}

#[test]
fn rpc_error_exit_codes() {
    let cases = [(-32601, 5), (-32602, 6), (-32603, 1), (-1, 1), (0, 1)];
    for (wire, expected) in cases {
        let script = format!(
            r#"{{"id":1,"error":{{"code":{wire},"message":"unknown method 'wall.ping'"}}}}"#
        );
        let daemon = fake_daemon(&[&[script.as_str()]]);
        let res = call(&daemon.sock, "wall.ping", &json!({}));
        let Err(error) = res else { panic!("no rpc error for {wire}") };
        assert_eq!(error.code(), expected, "wire {wire}");
        assert_eq!(error.message(), "unknown method 'wall.ping'", "wire {wire}");
    }
}

#[test]
fn local_error_exit_code() {
    let error = CliErr::local("picker is not running (launch skwd-wall first)");
    assert_eq!(error.code(), 1);
    assert_eq!(error.message(), "picker is not running (launch skwd-wall first)");
}

#[test]
fn call_eof_unreachable() {
    let daemon = fake_daemon(&[&[]]);
    let res = call(&daemon.sock, "wall.ping", &json!({}));
    assert!(matches!(res, Err(CliErr::Unreachable)));
    assert_eq!(CliErr::Unreachable.code(), 3);
}

#[test]
fn call_missing_socket() {
    let gone = std::env::temp_dir().join(format!("skwd-cli-gone-{}.sock", std::process::id()));
    let res = call(&gone, "wall.ping", &json!({}));
    assert!(matches!(res, Err(CliErr::Unreachable)));
}

#[test]
fn volume_verb_clamps() {
    let daemon = fake_daemon(&[&[r#"{"id":1,"result":{}}"#]]);
    let result =
        run_verb(&daemon.sock, "volume", vec!["150".into(), "--output".into(), "DP-1".into()]);
    assert_eq!(err_code(result), 0);
    let req = daemon.reqs.recv().expect("set_audio sent");
    assert_eq!(req["method"], "wall.set_audio");
    assert_eq!(req["params"]["volume"], 100);
    assert_eq!(req["params"]["outputs"], json!(["DP-1"]));
}

#[test]
fn volume_verb_bad_args() {
    let sock = std::env::temp_dir().join("skwd-cli-unused.sock");
    let result = run_verb(&sock, "volume", vec!["--output".into(), "DP-1".into()]);
    assert_eq!(err_code(result), 4);
}

#[test]
fn mute_verb_no_output() {
    let daemon = fake_daemon(&[&[r#"{"id":1,"result":{}}"#]]);
    let result = run_verb(&daemon.sock, "mute", vec![]);
    assert_eq!(err_code(result), 0);
    let req = daemon.reqs.recv().expect("set_audio sent");
    assert_eq!(req["method"], "wall.set_audio");
    assert_eq!(req["params"]["mute"], true);
    assert!(req["params"].get("outputs").is_none());
}

#[test]
fn apply_unknown_key() {
    let daemon = fake_daemon(&[&[r#"{"id":1,"result":{"wallpapers":[]}}"#], &[]]);
    let result = run_verb(&daemon.sock, "apply", vec!["no-such-wallpaper-xyz".into()]);
    assert_eq!(err_code(result), 2);
    let first = daemon.reqs.recv().expect("list requested");
    assert_eq!(first["method"], "wall.list");
    assert!(matches!(daemon.reqs.try_recv(), Err(mpsc::TryRecvError::Empty)));
}

#[test]
fn apply_file_path() {
    let dir = std::env::temp_dir().join(format!("skwd-cli-apply-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("w.png");
    std::fs::write(&file, b"x").unwrap();
    let canon = std::fs::canonicalize(&file).unwrap();
    let daemon = fake_daemon(&[&[r#"{"id":1,"result":{}}"#]]);
    let result = run_verb(
        &daemon.sock,
        "apply",
        vec![file.to_string_lossy().into_owned(), "--mute".into(), "--volume".into(), "150".into()],
    );
    assert_eq!(err_code(result), 0);
    let req = daemon.reqs.recv().expect("apply sent");
    assert_eq!(req["method"], "wall.apply");
    assert_eq!(req["params"]["type"], "static");
    assert_eq!(req["params"]["path"], canon.to_string_lossy().as_ref());
    assert_eq!(req["params"]["output"], "*");
    assert_eq!(req["params"]["mute"], true);
    assert_eq!(req["params"]["volume"], 100);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn next_verb_not_found() {
    let daemon = fake_daemon(&[
        &[r#"{"id":1,"result":{"ok":false}}"#],
        &[r#"{"id":1,"result":{"ok":true}}"#],
    ]);
    let first = run_verb(&daemon.sock, "next", vec![]);
    let second = run_verb(&daemon.sock, "next", vec!["-o".into(), "DP-2".into()]);
    assert_eq!(err_code(first), 2);
    assert_eq!(err_code(second), 0);
    assert_eq!(daemon.reqs.recv().unwrap()["params"]["output"], "*");
    let req = daemon.reqs.recv().unwrap();
    assert_eq!(req["method"], "wall.playlist.next");
    assert_eq!(req["params"]["output"], "DP-2");
}

#[test]
fn back_forward_verbs() {
    let daemon = fake_daemon(&[
        &[r#"{"id":1,"result":{"ok":false,"message":"no back history for *"}}"#],
        &[r#"{"id":1,"result":{"ok":true,"outputs":["DP-2"]}}"#],
    ]);
    let first = run_verb(&daemon.sock, "back", vec![]);
    let second = run_verb(&daemon.sock, "forward", vec!["-o".into(), "DP-2".into()]);
    assert_eq!(err_code(first), 2);
    assert_eq!(err_code(second), 0);
    let back = daemon.reqs.recv().unwrap();
    assert_eq!(back["method"], "wall.history.back");
    assert_eq!(back["params"]["output"], "*");
    let forward = daemon.reqs.recv().unwrap();
    assert_eq!(forward["method"], "wall.history.forward");
    assert_eq!(forward["params"]["output"], "DP-2");
}

#[test]
fn history_verb_defaults() {
    let daemon = fake_daemon(&[&[
        r#"{"id":1,"result":{"outputs":{"DP-1":{"pos":1,"entries":[{"ty":"static","path":"/a","we_id":"","mute":true,"volume":0},{"ty":"static","path":"/b","we_id":"","mute":true,"volume":0}]}}}}"#,
    ]]);
    let result = run_verb(&daemon.sock, "history", vec![]);
    assert_eq!(err_code(result), 0);
    let req = daemon.reqs.recv().unwrap();
    assert_eq!(req["method"], "wall.history.list");
    assert_eq!(req["params"]["output"], "*");
}

#[test]
fn watch_subscribes() {
    let daemon = fake_daemon(&[&[
        r#"{"event":"skwd.wall.applied","data":{"name":"a"}}"#,
        "",
        "not json",
        r#"{"id":9,"result":{}}"#,
    ]]);
    let result = watch(&daemon.sock, vec![]);
    assert_eq!(err_code(result), 0);
    let req = daemon.reqs.recv().expect("subscribe sent");
    assert_eq!(req["method"], "subscribe");
}

#[test]
fn workshop_video_uses_indexed_media_path() {
    let item = serde_json::from_value(
        json!({"key":"we:42", "type":"video", "video_file":"/workshop/42/movie.mp4"}),
    )
    .unwrap();
    assert_eq!(
        apply_params_for_item(&item, "/images", "/videos", "DP-1"),
        Some(json!({"type":"video", "path":"/workshop/42/movie.mp4", "output":"DP-1"}))
    );
}
