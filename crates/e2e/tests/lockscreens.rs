use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};
use skwd_e2e::{Sandbox, Walld, ffmpeg_still, wait_until};

const WAIT: Duration = Duration::from_secs(10);

fn session(name: &str) -> Sandbox {
    let mut sandbox = Sandbox::new(name);
    let stub = skwd_e2e::stub_renderer!();
    sandbox.set_env("SKWD_WALL_PAPER_STILL", &stub);
    sandbox.set_env("SKWD_FAKE_OUTPUTS", "DP-1:1920x1080,DP-2:2560x1440");
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") {
        sandbox.set_env("XDG_RUNTIME_DIR", &runtime.to_string_lossy());
    }
    for directory in ["config/swaylock", "config/DankMaterialShell", "state/noctalia"] {
        std::fs::create_dir_all(sandbox.root.join(directory)).unwrap();
    }
    std::fs::write(sandbox.root.join("config/swaylock/config"), "show-failed-attempts\n").unwrap();
    std::fs::write(
        sandbox.root.join("config/DankMaterialShell/settings.json"),
        r#"{"lockScreenWallpaperPath":"before.png","showSeconds":true}"#,
    )
    .unwrap();
    std::fs::write(
        sandbox.state().join("noctalia/settings.toml"),
        "config_version = 14\n[lockscreen]\nwallpaper = 'before.png'\nblurred_desktop = true\n",
    )
    .unwrap();
    stub_binaries(&mut sandbox);
    sandbox
}

fn stub_binaries(sandbox: &mut Sandbox) {
    use std::os::unix::fs::PermissionsExt;

    let bin = sandbox.root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    for name in ["hyprlock", "swaylock", "dms", "noctalia"] {
        let path = bin.join(name);
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    sandbox.set_env("PATH", &format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()));
}

fn fresh_session(name: &str) -> Sandbox {
    let mut sandbox = Sandbox::new(name);
    let stub = skwd_e2e::stub_renderer!();
    sandbox.set_env("SKWD_WALL_PAPER_STILL", &stub);
    sandbox.set_env("SKWD_FAKE_OUTPUTS", "DP-1:1920x1080");
    stub_binaries(&mut sandbox);
    sandbox
}

fn config(sandbox: &Sandbox, lock: &Value) -> Value {
    json!({
        "paths": {"wallpaper": sandbox.library()},
        "pickOnlyMode": false,
        "restoreOnStartup": false,
        "general": {"randomInterval": 0},
        "transition": {"enabled": false},
        "theme": {"policy": "off"},
        "lockScreens": lock,
    })
}

fn image(sandbox: &Sandbox, name: &str, colour: &str) -> PathBuf {
    let path = sandbox.library().join(name);
    assert!(ffmpeg_still(&path, &format!("color=c={colour}:s=96x64")));
    path
}

fn dms_image(sandbox: &Sandbox) -> String {
    std::fs::read(sandbox.root.join("config/DankMaterialShell/settings.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|settings| settings["lockScreenWallpaperPath"].as_str().map(ToOwned::to_owned))
        .unwrap_or_default()
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn fixed_image_restores_settings() {
    let sandbox = session("lock-fixed");
    let originals: Vec<_> = [
        sandbox.root.join("config/swaylock/config"),
        sandbox.root.join("config/DankMaterialShell/settings.json"),
        sandbox.state().join("noctalia/settings.toml"),
    ]
    .into_iter()
    .map(|path| {
        let bytes = std::fs::read(&path).unwrap();
        (path, bytes)
    })
    .collect();
    let source = image(&sandbox, "red.png", "red");
    let mut cfg = config(
        &sandbox,
        &json!({"mode":"static","image":source,"swaylock":true,"dms":true,"noctalia":true}),
    );
    sandbox.write_config(&cfg);
    let walld = Walld::start(&sandbox);
    assert!(wait_until(|| dms_image(&sandbox) != "before.png", WAIT), "{}", walld.log_contents());
    let exported = PathBuf::from(dms_image(&sandbox));
    let bytes = std::fs::read(&exported).unwrap();
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    let root = exported.parent().unwrap();
    assert_eq!(std::fs::read(root.join("wallpaper.png")).unwrap(), bytes);
    assert_eq!(
        std::fs::read(root.join("current")).unwrap(),
        exported.file_name().unwrap().as_encoded_bytes()
    );
    let sway = sandbox.root.join("config/swaylock/config");
    assert!(std::fs::read_to_string(&sway).unwrap().contains("show-failed-attempts"));
    let noctalia = sandbox.state().join("noctalia/settings.toml");
    assert!(wait_until(
        || std::fs::read_to_string(&noctalia).unwrap().contains("blurred_desktop = false"),
        WAIT
    ));
    for (path, bytes) in &originals {
        let backup = path
            .with_file_name(format!("{}.skwd-backup", path.file_name().unwrap().to_string_lossy()));
        assert_eq!(std::fs::read(backup).unwrap(), *bytes);
    }
    cfg["lockScreens"]["mode"] = json!("off");
    sandbox.write_config(&cfg);
    assert!(wait_until(|| dms_image(&sandbox) == "before.png", WAIT));
    assert!(wait_until(
        || std::fs::read_to_string(&sway).unwrap() == "show-failed-attempts\n",
        WAIT
    ));
    assert!(wait_until(
        || std::fs::read_to_string(&noctalia).unwrap().contains("blurred_desktop = true"),
        WAIT
    ));
    for (path, bytes) in originals {
        let backup = path
            .with_file_name(format!("{}.skwd-backup", path.file_name().unwrap().to_string_lossy()));
        assert_eq!(std::fs::read(backup).unwrap(), bytes);
    }
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn follow_tracks_last_apply() {
    let sandbox = session("lock-follow");
    sandbox.write_config(&config(&sandbox, &json!({"mode":"follow","dms":true})));
    let red = image(&sandbox, "red.png", "red");
    let blue = image(&sandbox, "blue.png", "blue");
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    for (id, path, output) in [(1, red, "DP-1"), (2, blue, "DP-2")] {
        let before = dms_image(&sandbox);
        let reply = client
            .call("wall.apply", json!({"type":"static","path":path,"output":output}), id)
            .unwrap();
        assert!(reply.get("error").is_none(), "{reply}");
        assert!(wait_until(|| dms_image(&sandbox) != before, WAIT), "{}", walld.log_contents());
    }
    let exported = PathBuf::from(dms_image(&sandbox));
    let bytes = std::fs::read(&exported).unwrap();
    std::fs::remove_dir_all(sandbox.library()).unwrap();
    std::fs::remove_dir_all(sandbox.cache()).unwrap();
    assert_eq!(std::fs::read(exported).unwrap(), bytes);
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn off_leaves_configs_untouched() {
    let sandbox = session("lock-off");
    sandbox.write_config(&config(&sandbox, &json!({"mode":"off","dms":true,"swaylock":true})));
    let walld = Walld::start(&sandbox);
    assert!(walld.responsive());
    assert_eq!(dms_image(&sandbox), "before.png");
    assert_eq!(
        std::fs::read_to_string(sandbox.root.join("config/swaylock/config")).unwrap(),
        "show-failed-attempts\n"
    );
    assert!(!sandbox.root.join("data/skwd-wall-v2/lockscreens").exists());
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn set_restores_dms() {
    let sandbox = session("lock-set-dms");
    let source = image(&sandbox, "red.png", "red");
    sandbox.write_config(&config(&sandbox, &json!({"mode":"static","image":source})));
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let reply = client
        .call(wall_proto::rpc::LOCKSCREEN_SET, json!({"target":"dms","enabled":true}), 1)
        .unwrap();
    assert_eq!(reply["result"]["ok"], true, "{reply}");
    let exported = PathBuf::from(dms_image(&sandbox));
    assert!(exported.starts_with(sandbox.root.join("data/skwd-wall-v2/lockscreens")));
    assert!(exported.is_file());
    let saved: Value =
        serde_json::from_slice(&std::fs::read(sandbox.config_path()).unwrap()).unwrap();
    assert_eq!(saved["lockScreens"]["mode"], "static");
    assert_eq!(saved["lockScreens"]["dms"], true);
    let reply = client
        .call(wall_proto::rpc::LOCKSCREEN_SET, json!({"target":"dms","enabled":false}), 2)
        .unwrap();
    assert_eq!(reply["result"]["ok"], true, "{reply}");
    assert_eq!(dms_image(&sandbox), "before.png");
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn set_failure_keeps_config() {
    let sandbox = session("lock-set-fail");
    let config = config(&sandbox, &json!({"mode":"off"}));
    sandbox.write_config(&config);
    let before = std::fs::read(sandbox.config_path()).unwrap();
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let reply = client
        .call(wall_proto::rpc::LOCKSCREEN_SET, json!({"target":"dms","enabled":true}), 1)
        .unwrap();
    assert!(reply.get("error").is_some(), "{reply}");
    assert_eq!(std::fs::read(sandbox.config_path()).unwrap(), before);
    assert_eq!(dms_image(&sandbox), "before.png");
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn set_ignores_dormant_target() {
    let sandbox = session("lock-set-dormant");
    let source = image(&sandbox, "red.png", "red");
    let hypr = sandbox.root.join("config/hypr/hyprlock.conf");
    std::fs::create_dir_all(hypr.parent().unwrap()).unwrap();
    let malformed = "background {\n    path = one.png\n    path = two.png\n}\n";
    std::fs::write(&hypr, malformed).unwrap();
    sandbox.write_config(&config(
        &sandbox,
        &json!({
            "mode":"off","image":source.clone(),"hyprlock":true
        }),
    ));
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let applied = client
        .call(
            wall_proto::rpc::WALL_APPLY,
            json!({"type":"static","path":source,"output":"DP-1"}),
            1,
        )
        .unwrap();
    assert!(applied.get("error").is_none(), "{applied}");
    let reply = client
        .call(wall_proto::rpc::LOCKSCREEN_SET, json!({"target":"swaylock","enabled":true}), 2)
        .unwrap();
    assert_eq!(reply["result"]["ok"], true, "{reply}");
    assert_eq!(std::fs::read_to_string(&hypr).unwrap(), malformed);
    let saved: Value =
        serde_json::from_slice(&std::fs::read(sandbox.config_path()).unwrap()).unwrap();
    assert_eq!(saved["lockScreens"]["hyprlock"], false);
    assert_eq!(saved["lockScreens"]["swaylock"], true);
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn set_rejects_stale_config() {
    let mut sandbox = session("lock-set-stale");
    let hypr = sandbox.root.join("config/hypr/hyprlock.conf");
    std::fs::create_dir_all(hypr.parent().unwrap()).unwrap();
    std::fs::write(&hypr, "background { path = old.png }\n").unwrap();
    sandbox.write_config(&config(&sandbox, &json!({"mode":"off"})));
    sandbox.set_env("PATH", &sandbox.root.join("empty-bin").to_string_lossy());
    let before = std::fs::read(sandbox.config_path()).unwrap();
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let reply = client
        .call(wall_proto::rpc::LOCKSCREEN_SET, json!({"target":"hyprlock","enabled":true}), 1)
        .unwrap();
    assert!(reply.get("error").is_some(), "{reply}");
    assert_eq!(std::fs::read(sandbox.config_path()).unwrap(), before);
    assert_eq!(std::fs::read_to_string(hypr).unwrap(), "background { path = old.png }\n");
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn set_creates_fresh_configs() {
    let sandbox = fresh_session("lock-set-fresh");
    let source = image(&sandbox, "red.png", "red");
    sandbox.write_config(&config(&sandbox, &json!({"mode":"off"})));
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let applied = client
        .call(
            wall_proto::rpc::WALL_APPLY,
            json!({"type":"static","path":source,"output":"DP-1"}),
            1,
        )
        .unwrap();
    assert!(applied.get("error").is_none(), "{applied}");
    for (id, target) in ["hyprlock", "swaylock", "dms", "noctalia"].into_iter().enumerate() {
        let reply = client
            .call(
                wall_proto::rpc::LOCKSCREEN_SET,
                json!({"target":target,"enabled":true}),
                id as u64 + 2,
            )
            .unwrap();
        assert_eq!(reply["result"]["ok"], true, "{target}: {reply}");
    }
    let hyprlock = sandbox.root.join("config/hypr/hyprlock.conf");
    let swaylock = sandbox.root.join("config/swaylock/config");
    let dms = sandbox.root.join("config/DankMaterialShell/settings.json");
    let noctalia = sandbox.state().join("noctalia/settings.toml");
    let text = std::fs::read_to_string(&hyprlock).unwrap();
    assert!(
        text.contains("background {")
            && text.contains("input-field {")
            && text.contains("source = ")
    );
    assert!(std::fs::read_to_string(&swaylock).unwrap().contains("image="));
    assert!(std::fs::read_to_string(&dms).unwrap().contains("lockScreenWallpaperPath"));
    assert!(std::fs::read_to_string(&noctalia).unwrap().contains("wallpaper = "));
    let saved: Value =
        serde_json::from_slice(&std::fs::read(sandbox.config_path()).unwrap()).unwrap();
    assert_eq!(saved["lockScreens"]["mode"], "follow");
    for (id, target) in ["hyprlock", "swaylock", "dms", "noctalia"].into_iter().enumerate() {
        let reply = client
            .call(
                wall_proto::rpc::LOCKSCREEN_SET,
                json!({"target":target,"enabled":false}),
                id as u64 + 6,
            )
            .unwrap();
        assert_eq!(reply["result"]["ok"], true, "{target}: {reply}");
    }
    for path in [hyprlock, swaylock, dms, noctalia] {
        assert!(!path.exists(), "{}", path.display());
    }
}
