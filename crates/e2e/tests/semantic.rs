use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use serde_json::json;
use skwd_e2e::{Sandbox, Walld, ffmpeg_still, wait_until};

#[test]
fn disabling_semantic_search_stops_indexing_and_reenable_restarts_it() {
    let mut sandbox = Sandbox::new("semantic");
    let helper = sandbox.root.join("lens-fixture");
    let launched = sandbox.root.join("launches");
    fs::write(&helper, "#!/usr/bin/python3\nimport os,sys,signal\nsys.stdin.read()\nwith open(os.environ['SEMANTIC_LAUNCHES'], 'a') as f: f.write(str(os.getpid())+'\\n')\nsignal.pause()\n").unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
    let assets = sandbox.root.join("model");
    fs::create_dir_all(assets.join("runtime")).unwrap();
    fs::write(assets.join("semantic-pack.json"), "{}").unwrap();
    fs::write(assets.join("runtime/libonnxruntime.so"), "fixture").unwrap();
    sandbox.set_env("SKWD_LENS_BIN", helper.to_str().unwrap());
    sandbox.set_env("SKWD_LENS_HOME", assets.to_str().unwrap());
    sandbox.set_env("SKWD_LENS_INDEX", sandbox.root.join("index.sidx").to_str().unwrap());
    sandbox.set_env("SEMANTIC_LAUNCHES", launched.to_str().unwrap());
    assert!(ffmpeg_still(&sandbox.library().join("one.png"), "color=c=red:s=32x32"));
    let mut config = json!({
        "paths": {"wallpaper": sandbox.library(), "videoWallpaper": sandbox.library()},
        "restoreOnStartup": false, "pickOnlyMode": true,
        "semantic": {"enabled": false},
        "effects": {"autoRecolor": false, "autoTheme": ""},
    });
    sandbox.write_config(&config);
    let walld = Walld::start(&sandbox);
    assert!(
        wait_until(|| skwd_e2e::db_count(&sandbox.sqlite_path()) >= 1, Duration::from_secs(15)),
        "{}",
        walld.log_contents()
    );
    std::thread::sleep(Duration::from_millis(600));
    assert!(!launched.exists(), "disabled startup launched an indexer");
    config["semantic"]["enabled"] = json!(true);
    sandbox.write_config(&config);
    assert!(wait_until(|| launched.exists(), Duration::from_secs(10)), "{}", walld.log_contents());
    let first: u32 =
        fs::read_to_string(&launched).unwrap().lines().next().unwrap().parse().unwrap();
    assert!(std::path::Path::new(&format!("/proc/{first}")).exists());
    config["semantic"]["enabled"] = json!(false);
    sandbox.write_config(&config);
    assert!(
        wait_until(
            || !std::path::Path::new(&format!("/proc/{first}")).exists(),
            Duration::from_secs(10)
        ),
        "disabled search left the indexer alive"
    );
    assert!(ffmpeg_still(&sandbox.library().join("two.png"), "color=c=blue:s=32x32"));
    assert!(wait_until(
        || skwd_e2e::db_count(&sandbox.sqlite_path()) >= 2,
        Duration::from_secs(15)
    ));
    std::thread::sleep(Duration::from_millis(600));
    assert_eq!(fs::read_to_string(&launched).unwrap().lines().count(), 1);
    config["semantic"]["enabled"] = json!(true);
    sandbox.write_config(&config);
    assert!(wait_until(
        || fs::read_to_string(&launched).unwrap().lines().count() == 2,
        Duration::from_secs(10)
    ));
    let second = fs::read_to_string(&launched).unwrap().lines().last().unwrap().to_string();
    let response =
        walld.client().call("task.control", json!({"id": "semantic-index", "action": "pause"}), 8);
    assert!(response.is_some_and(|value| value.get("result").is_some()));
    assert!(wait_until(
        || fs::read_to_string(format!("/proc/{second}/status")).is_ok_and(|status| status
            .lines()
            .any(|line| line.starts_with("State:") && line.contains("T (stopped)"))),
        Duration::from_secs(5)
    ));
    config["semantic"]["enabled"] = json!(false);
    sandbox.write_config(&config);
    assert!(wait_until(
        || !std::path::Path::new(&format!("/proc/{second}")).exists(),
        Duration::from_secs(10)
    ));
    assert!(walld.responsive());
}
