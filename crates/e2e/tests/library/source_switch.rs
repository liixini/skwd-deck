use super::*;
use std::os::unix::fs::PermissionsExt;

fn wait_keys(client: &mut Client, expected: &[&str]) {
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        let response = client.call("wall.list", json!({}), 1).expect("library response");
        let mut keys = response["result"]["wallpapers"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|row| row["key"].as_str())
            .filter(|key| key.starts_with("static:"))
            .collect::<Vec<_>>();
        keys.sort_unstable();
        if keys == expected {
            return;
        }
        assert!(Instant::now() < deadline, "expected {expected:?}, got {keys:?}");
        std::thread::sleep(Duration::from_millis(150));
    }
}

fn wait_index(path: &Path, expected: &[&str]) {
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        let request =
            fs::read(path).ok().and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
        if let Some(request) = request {
            let mut keys = request["entries"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|row| row["key"].as_str())
                .collect::<Vec<_>>();
            keys.sort_unstable();
            keys.dedup();
            if keys == expected {
                return;
            }
        }
        assert!(Instant::now() < deadline, "semantic catalog did not converge to {expected:?}");
        std::thread::sleep(Duration::from_millis(150));
    }
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn source_switch_replaces_watches_and_semantic_catalog() {
    let mut sandbox = Sandbox::new("source-switch");
    let first = sandbox.library();
    let second = sandbox.root.join("Imágenes nuevas 桜");
    let empty = sandbox.root.join("empty");
    for directory in [&second, &empty] {
        fs::create_dir_all(directory).unwrap();
    }
    assert!(ffmpeg_still(&first.join("old.png"), "color=c=red:s=96x64"));
    assert!(ffmpeg_still(&second.join("new.png"), "color=c=blue:s=96x64"));
    fs::copy(first.join("old.png"), first.join("shared.png")).unwrap();
    fs::copy(second.join("new.png"), second.join("shared.png")).unwrap();
    let timestamp = std::time::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    for directory in [&first, &second] {
        fs::File::options()
            .write(true)
            .open(directory.join("shared.png"))
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(timestamp))
            .unwrap();
    }
    let helper = sandbox.root.join("lens-fixture");
    fs::write(
        &helper,
        r"#!/usr/bin/python3
import json, os, pathlib, sys
request = json.load(sys.stdin)
index = pathlib.Path(sys.argv[sys.argv.index('--index') + 1])
index.parent.mkdir(parents=True, exist_ok=True)
temporary = index.with_suffix('.tmp')
temporary.write_text(json.dumps(request))
os.replace(temporary, index)
pathlib.Path(str(index) + '.fingerprint').write_text(str(request['fingerprint']))
",
    )
    .unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
    let manifest = sandbox.root.join("manifest.json");
    fs::write(&manifest, r#"{"id":"fixture","version":"1"}"#).unwrap();
    let runtime = sandbox.root.join("runtime.so");
    fs::write(&runtime, "fixture").unwrap();
    let index = sandbox.root.join("captured-index.json");
    for (key, path) in [
        ("SKWD_LENS_BIN", &helper),
        ("SKWD_LENS_MANIFEST", &manifest),
        ("SKWD_LENS_ORT_DYLIB", &runtime),
        ("SKWD_LENS_INDEX", &index),
    ] {
        sandbox.set_env(key, path.to_str().unwrap());
    }
    let mut config = json!({"paths":{"wallpaper":first,"videoWallpaper":empty,"steamWorkshop":empty,"steamWeAssets":empty},"pickOnlyMode":true,"restoreOnStartup":false,"effects":{"autoRecolor":false}});
    sandbox.write_config(&config);
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    wait_keys(&mut client, &["static:old.png", "static:shared.png"]);
    wait_index(&index, &["static:old.png", "static:shared.png"]);
    let before: Value = serde_json::from_slice(&fs::read(&index).unwrap()).unwrap();
    let old_shared = before["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["key"] == "static:shared.png")
        .unwrap();
    let old_thumb = fs::read(old_shared["path"].as_str().unwrap()).unwrap();
    config["paths"]["wallpaper"] = json!(second);
    let staged = sandbox.config_path().with_extension("new");
    fs::write(&staged, serde_json::to_vec(&config).unwrap()).unwrap();
    fs::rename(&staged, sandbox.config_path()).unwrap();
    wait_keys(&mut client, &["static:new.png", "static:shared.png"]);
    wait_index(&index, &["static:new.png", "static:shared.png"]);
    let after: Value = serde_json::from_slice(&fs::read(&index).unwrap()).unwrap();
    let new_shared = after["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["key"] == "static:shared.png")
        .unwrap();
    assert_ne!(
        old_thumb,
        fs::read(new_shared["path"].as_str().unwrap()).unwrap(),
        "new source reused old thumbnail"
    );
    assert_ne!(
        old_shared["fingerprint"], new_shared["fingerprint"],
        "new source reused old embedding"
    );
    fs::copy(second.join("new.png"), second.join("added.png")).unwrap();
    wait_keys(&mut client, &["static:added.png", "static:new.png", "static:shared.png"]);
    wait_index(&index, &["static:added.png", "static:new.png", "static:shared.png"]);
    fs::copy(first.join("old.png"), first.join("retired.png")).unwrap();
    std::thread::sleep(Duration::from_secs(3));
    wait_keys(&mut client, &["static:added.png", "static:new.png", "static:shared.png"]);
    config["paths"]["wallpaper"] = json!(empty);
    sandbox.write_config(&config);
    wait_keys(&mut client, &[]);
    let deadline = Instant::now() + Duration::from_secs(45);
    while index.exists() || PathBuf::from(format!("{}.fingerprint", index.display())).exists() {
        assert!(Instant::now() < deadline, "empty library retained semantic index");
        std::thread::sleep(Duration::from_millis(150));
    }
    config["paths"]["wallpaper"] = json!(first);
    sandbox.write_config(&config);
    wait_keys(&mut client, &["static:old.png", "static:retired.png", "static:shared.png"]);
    wait_index(&index, &["static:old.png", "static:retired.png", "static:shared.png"]);
}
