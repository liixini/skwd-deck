use paper_control::{Assignment, SourceKind};
use serde_json::{Map, Value, json};
use skwd_e2e::{
    Client, FakePlasma, FakePlugin, Sandbox, Walld, child_pids, err_message, ffmpeg_still,
    wait_until,
};
use std::time::Duration;

const STUB: &str = "fake_renderer";

#[path = "plasma/transitions.rs"]
mod transitions;

#[path = "plasma/preparation.rs"]
mod preparation;

fn plasma_session(name: &str) -> (Sandbox, FakePlasma) {
    let mut sandbox = Sandbox::new(name);
    let plasma = FakePlasma::install(
        &mut sandbox,
        env!("CARGO_BIN_EXE_fake_qdbus"),
        env!("CARGO_BIN_EXE_fake_kconfig"),
    );
    let stub = skwd_e2e::stub_renderer!();
    sandbox.set_env("SKWD_WALL_PAPER_STILL", &stub);
    sandbox.set_env("SKWD_WALL_PAPER_VK", &stub);
    sandbox.set_env("SKWD_FAKE_OUTPUTS", "DP-1:1920x1080,DP-2:2560x1440");
    let we = sandbox.root.join("we");
    let scene = we.join("scene-a");
    std::fs::create_dir_all(&scene).expect("scene fixture");
    std::fs::write(scene.join("project.json"), r#"{"type":"scene","title":"scene-a"}"#)
        .expect("project.json");
    std::fs::write(scene.join("scene.pkg"), b"scene fixture").expect("scene.pkg");
    sandbox.write_config(&json!({
        "paths": {"wallpaper": sandbox.library(), "videoWallpaper": sandbox.library(), "steamWorkshop": we, "steamWeAssets": we},
        "pickOnlyMode": false,
        "restoreOnStartup": false,
        "general": {"randomInterval": 0},
        "effects": {"autoRecolor": false, "autoTheme": ""},
        "theme": {"policy": "off"},
        "transition": {"enabled": false},
        "playback": {"fullscreen": true, "fullscreenScope": "display", "resumeDelay": 0},
    }));
    (sandbox, plasma)
}

fn still(sandbox: &Sandbox, name: &str, color: &str) -> String {
    let path = sandbox.library().join(name);
    assert!(ffmpeg_still(&path, &format!("color=c={color}:s=320x180")), "{name} fixture");
    path.to_string_lossy().into_owned()
}

fn call(walld: &Walld, client: &mut Client, method: &str, params: Value) -> Value {
    let response = client
        .call(method, params, 1)
        .unwrap_or_else(|| panic!("{method}: no response\n{}", walld.log_contents()));
    assert!(response.get("error").is_none(), "{method}: {response}\n{}", walld.log_contents());
    response
}

fn assignment(entry: &Value) -> Assignment {
    serde_json::from_value(entry["assignment"].clone()).expect("Plasma assignment")
}

fn source(entry: &Value) -> (SourceKind, String) {
    let assignment = assignment(entry);
    (assignment.source.kind, assignment.source.path)
}

fn scripted(plasma: &FakePlasma) -> Map<String, Value> {
    plasma.scripts().pop().expect("a Plasma script ran")
}

fn backend(walld: &Walld) -> String {
    walld.client().call("status", json!({}), 2).expect("status")["result"]["playback"]
        ["window_state_backend"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

fn restore_session(name: &str) -> (Sandbox, FakePlasma, String) {
    let (sandbox, plasma) = plasma_session(name);
    let image = still(&sandbox, "restore.png", "red");
    {
        let walld = Walld::start(&sandbox);
        let mut client = walld.client();
        let reply = client
            .call("wall.apply", json!({"type":"static","path":image}), 9)
            .unwrap_or_else(|| panic!("wall.apply: no response\n{}", walld.log_contents()));
        assert!(reply.get("error").is_none(), "{reply}\n{}", walld.log_contents());
    }
    let mut config: Value =
        serde_json::from_slice(&std::fs::read(sandbox.config_path()).unwrap()).unwrap();
    config["restoreOnStartup"] = json!(true);
    sandbox.write_config(&config);
    (sandbox, plasma, image)
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn local_we_clears_activities() {
    let (sandbox, _plasma) = plasma_session("activity-we-delete");
    let source = sandbox.root.join("we");
    let actual = sandbox.root.join("native/710007");
    std::fs::create_dir_all(&actual).unwrap();
    std::fs::write(
        actual.join("project.json"),
        r#"{"type":"scene","title":"Local scene","file":"scene.json","preview":"preview.png"}"#,
    )
    .unwrap();
    std::fs::write(actual.join("scene.json"), "{}").unwrap();
    assert!(ffmpeg_still(&actual.join("preview.png"), "color=c=blue:s=96x64"));
    std::os::unix::fs::symlink(&actual, source.join("710007")).unwrap();
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    assert!(wait_until(
        || client
            .call("wall.list", json!({}), 1)
            .and_then(|response| response["result"]["wallpapers"].as_array().cloned())
            .is_some_and(|rows| rows.iter().any(|row| row["key"] == "we:710007")),
        Duration::from_secs(15)
    ));
    let keep = json!({"wallpaper":{"path":"/unrelated/image.png","we_id":""}});
    let saved = json!({
        "activity-a": {
            "DP-1":{"wallpaper":{"path":actual.join("scene.json"),"we_id":""}},
            "DP-2":keep
        },
        "activity-b": {"HDMI-1":{"wallpaper":{"path":actual,"we_id":""}}}
    });
    let assignments = sandbox.root.join("plasma/assignments.json");
    std::fs::write(&assignments, saved.to_string()).unwrap();
    call(&walld, &mut client, "wall.remove", json!({"we_id":"710007","unsubscribe":false}));
    let after: Value = serde_json::from_slice(&std::fs::read(assignments).unwrap()).unwrap();
    assert!(after["activity-a"].get("DP-1").is_none());
    assert_eq!(after["activity-a"]["DP-2"], keep);
    assert!(after["activity-b"].get("HDMI-1").is_none());
    assert!(actual.is_dir());
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn plasma_apply_scripts_every_output_and_waits_for_its_frames() {
    let (sandbox, plasma) = plasma_session("plasma-script");
    let first = still(&sandbox, "a.png", "red");
    let second = still(&sandbox, "b.png", "green");
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();

    call(&walld, &mut client, "wall.apply", json!({"type": "static", "path": first}));
    let scripts = plasma.scripts();
    assert_eq!(scripts.len(), 1, "{:?}", plasma.calls());
    for (output, width, height) in [("DP-1", 1920, 1080), ("DP-2", 2560, 1440)] {
        let entry = &scripts[0][output];
        assert_eq!(assignment(entry).outputs, [output], "{entry}");
        assert_eq!(source(entry), (SourceKind::Static, first.clone()), "{entry}");
        assert_eq!(
            (entry["width"].as_u64(), entry["height"].as_u64()),
            (Some(width), Some(height))
        );
        assert!(entry["presentationId"].as_str().is_some_and(|id| !id.is_empty()), "{entry}");
    }
    assert!(child_pids(walld.pid(), STUB).is_empty(), "Plasma started a native renderer");

    plasma.fail_presentations("decoder refused the file");
    let response = client
        .call("wall.apply", json!({"type": "static", "path": second}), 3)
        .expect("walld response");
    assert!(
        err_message(Some(&response)).contains("decoder refused the file"),
        "{response}\n{}",
        walld.log_contents()
    );

    plasma.confirm_presentations();
    call(&walld, &mut client, "wall.apply", json!({"type": "static", "path": first}));
    assert!(walld.responsive());
    assert!(!walld.log_contents().contains("panicked"));
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn plasma_still_on_one_output_keeps_the_other_outputs_scene() {
    let (sandbox, plasma) = plasma_session("plasma-per-output");
    let image = still(&sandbox, "a.png", "red");
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();

    call(&walld, &mut client, "wall.apply", json!({"type": "we", "we_id": "scene-a"}));
    call(
        &walld,
        &mut client,
        "wall.set_audio",
        json!({"mute": false, "volume": 40, "outputs": ["DP-1"]}),
    );
    call(
        &walld,
        &mut client,
        "wall.apply",
        json!({"type": "static", "path": image, "output": "DP-1"}),
    );

    let script = scripted(&plasma);
    assert_eq!(source(&script["DP-1"]), (SourceKind::Static, image.clone()), "{script:?}");
    let still = assignment(&script["DP-1"]);
    assert_eq!((still.mute, still.volume), (false, 40), "{script:?}");
    assert_eq!(source(&script["DP-2"]).0, SourceKind::WallpaperEngine, "{script:?}");

    let outputs = sandbox.outputs_json();
    assert_eq!(outputs["DP-1"]["type"], "static", "{outputs}");
    assert_eq!((&outputs["DP-1"]["mute"], &outputs["DP-1"]["volume"]), (&json!(false), &json!(40)));
    assert_eq!(
        (&outputs["DP-2"]["type"], &outputs["DP-2"]["we_id"]),
        (&json!("we"), &json!("scene-a"))
    );
    assert!(child_pids(walld.pid(), STUB).is_empty(), "Plasma started a native renderer");
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn subscriptions_preserve_activity_config() {
    let (sandbox, plasma) = plasma_session("plasma-channel");
    let first = still(&sandbox, "a.png", "red");
    let second = still(&sandbox, "b.png", "green");
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let mut left = FakePlugin::connect(&sandbox, "DP-1");
    let mut right = FakePlugin::connect(&sandbox, "DP-2");
    left.subscribe();
    right.subscribe();
    call(&walld, &mut client, "wall.apply", json!({"type":"static","path":first}));
    call(&walld, &mut client, "wall.apply", json!({"type":"static","path":second,"output":"DP-1"}));
    assert_eq!(plasma.scripts().len(), 2);
    assert_eq!(source(&scripted(&plasma)["DP-1"]), (SourceKind::Static, second));
    assert_eq!(source(&scripted(&plasma)["DP-2"]), (SourceKind::Static, first));
    assert!(left.entries().is_empty());
    assert!(right.entries().is_empty());
    assert!(left.lines().iter().all(|line| line["capabilities"] == json!([])));
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn plugin_restart_keeps_config() {
    let (sandbox, plasma) = plasma_session("plasma-reconnect");
    let first = still(&sandbox, "a.png", "red");
    let second = still(&sandbox, "b.png", "green");
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    call(&walld, &mut client, "wall.apply", json!({"type":"static","path":first}));
    let plugins = [FakePlugin::connect(&sandbox, "DP-1"), FakePlugin::connect(&sandbox, "DP-2")];
    assert!(wait_until(|| backend(&walld) == "plasma", Duration::from_secs(5)));
    drop(plugins);
    assert!(wait_until(|| backend(&walld) != "plasma", Duration::from_secs(5)));
    call(&walld, &mut client, "wall.apply", json!({"type":"static","path":second}));
    assert_eq!(plasma.scripts().len(), 2);
    let mut plugin = FakePlugin::connect(&sandbox, "DP-1");
    plugin.subscribe();
    assert!(plugin.wait(|lines| !lines.is_empty()));
    assert!(plugin.entries().is_empty());
    assert_eq!(source(&scripted(&plasma)["DP-1"]), (SourceKind::Static, second));
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn plasma_pauses_reach_only_their_output() {
    let (sandbox, plasma) = plasma_session("plasma-pause");
    let image = still(&sandbox, "a.png", "red");
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let mut left = FakePlugin::connect(&sandbox, "DP-1");
    let mut right = FakePlugin::connect(&sandbox, "DP-2");
    let paused = |plugin: &FakePlugin, expected: bool| {
        plugin.wait(|lines| lines.last().is_some_and(|line| line["paused"] == expected))
    };
    assert!(
        paused(&left, false) && paused(&right, false),
        "{:?} {:?}",
        left.lines(),
        right.lines()
    );

    left.report(true);
    assert!(paused(&left, true), "{:?}", left.lines());
    assert!(
        wait_until(
            || {
                walld.client().call("status", json!({}), 3).is_some_and(|status| {
                    status["result"]["playback"]["outputs"] == json!(["DP-1"])
                })
            },
            Duration::from_secs(5)
        ),
        "{}",
        walld.log_contents()
    );
    assert!(right.lines().iter().all(|line| line["paused"] == false), "{:?}", right.lines());
    assert!(left.lines().iter().all(|line| line["capabilities"] == json!([])));
    left.report(false);
    assert!(paused(&left, false), "{:?}", left.lines());

    call(&walld, &mut client, "wall.apply", json!({"type": "static", "path": image}));
    left.subscribe();
    right.subscribe();
    let scripts = plasma.scripts().len();
    call(&walld, &mut client, "wall.set_paused", json!({"output":"DP-2","paused":true}));
    let saved = scripted(&plasma);
    assert_eq!(saved["DP-2"]["manualPaused"], true);
    assert_eq!(saved["DP-2"]["paused"], true);
    assert_eq!(saved["DP-1"]["manualPaused"], false);
    assert!(plasma.scripts().len() > scripts);
    assert!(left.entries().is_empty() && right.entries().is_empty());
}

fn lock_screen_plugin(sandbox: &Sandbox) -> String {
    std::process::Command::new(sandbox.root.join("plasma/bin/kreadconfig6"))
        .arg("--file")
        .arg(sandbox.root.join("config/kscreenlockerrc"))
        .args(["--group", "Greeter", "--key", "WallpaperPlugin"])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default()
}

fn set_lock_screen_mode(sandbox: &Sandbox, config: &mut Value, mode: &str) {
    config["plasma"]["lockScreen"]["mode"] = json!(mode);
    sandbox.write_config(config);
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn plasma_lock_screen_off_gives_the_lock_screen_back() {
    let (sandbox, plasma) = plasma_session("plasma-lock-screen-release");
    let image = still(&sandbox, "lock.png", "red");
    let lockrc = sandbox.root.join("config/kscreenlockerrc");
    let mut config: Value =
        serde_json::from_str(&std::fs::read_to_string(sandbox.config_path()).expect("config"))
            .expect("config json");
    config["plasma"] = json!({"lockScreen": {"mode": "static", "image": image}});
    sandbox.write_config(&config);
    let walld = Walld::start(&sandbox);
    let selected = |plugin: &str| {
        wait_until(|| lock_screen_plugin(&sandbox) == plugin, Duration::from_secs(10))
    };

    assert!(selected("org.skwd.wall.plasma"), "no takeover\n{}", walld.log_contents());
    set_lock_screen_mode(&sandbox, &mut config, "off");
    assert!(selected(""), "Plasma default not restored\n{}", walld.log_contents());
    let released = std::fs::read_to_string(&lockrc).unwrap_or_default();
    assert!(!released.lines().any(|line| line.starts_with("WallpaperPlugin=")), "{released}");

    std::fs::write(&lockrc, "[Greeter]\nWallpaperPlugin=org.kde.slideshow\n").expect("user choice");
    let original = std::fs::read(&lockrc).unwrap();
    set_lock_screen_mode(&sandbox, &mut config, "static");
    assert!(selected("org.skwd.wall.plasma"), "no second takeover\n{}", walld.log_contents());
    let backup = sandbox.root.join("config/kscreenlockerrc.skwd-backup");
    assert_eq!(std::fs::read(&backup).unwrap(), original);
    set_lock_screen_mode(&sandbox, &mut config, "off");
    assert!(selected("org.kde.slideshow"), "user plugin not restored\n{}", walld.log_contents());
    assert_eq!(std::fs::read(&backup).unwrap(), original);

    assert!(
        wait_until(
            || {
                plasma
                    .calls()
                    .iter()
                    .filter(|args| args.iter().any(|arg| arg == "org.kde.screensaver.configure"))
                    .count()
                    >= 4
            },
            Duration::from_secs(10)
        ),
        "{:?}",
        plasma.calls()
    );
    let kconfig =
        std::fs::read_to_string(sandbox.root.join("plasma/kconfig.log")).unwrap_or_default();
    assert!(
        kconfig.contains("PreviousWallpaperPlugin"),
        "walld bypassed the KConfig shim: {kconfig}"
    );
    assert!(walld.responsive());
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn compositor_protocol_overrides_stale_desktop_names() {
    for (index, desktop) in ["KDE", "niri:KDE", "GNOME:Plasma", ""].into_iter().enumerate() {
        let mut sandbox = Sandbox::new(&format!("desktop-stale-{index}"));
        sandbox.set_env("XDG_CURRENT_DESKTOP", desktop);
        sandbox.set_env("KDE_FULL_SESSION", "true");
        sandbox.set_env("XDG_DATA_DIRS", &sandbox.root.join("empty").to_string_lossy());
        sandbox.set_env("SKWD_FAKE_OUTPUTS", "DP-1:1920x1080");
        let stub = skwd_e2e::stub_renderer!();
        sandbox.set_env("SKWD_WALL_PAPER_STILL", &stub);
        sandbox.write_config(&json!({
            "paths": {"wallpaper": sandbox.library()},
            "restoreOnStartup": false,
            "general": {"randomInterval": 0},
            "theme": {"policy": "off"},
            "transition": {"enabled": false},
            "plasma": {"lockScreen": {"mode": "follow"}},
        }));
        if index % 2 == 1 {
            let plugin = sandbox.root.join("data/plasma/wallpapers/org.skwd.wall.plasma");
            std::fs::create_dir_all(&plugin).unwrap();
            std::fs::write(plugin.join("metadata.json"), "{}").unwrap();
        }
        let image = still(&sandbox, "stale.png", "red");
        let _wayland = skwd_e2e::FakeWayland::start(
            &mut sandbox,
            &["zwlr_layer_shell_v1", "org_kde_kwin_server_decoration_manager"],
        );
        let walld = Walld::start(&sandbox);
        call(&walld, &mut walld.client(), "wall.apply", json!({"type": "static", "path": image}));
        assert!(
            wait_until(|| !child_pids(walld.pid(), STUB).is_empty(), Duration::from_secs(5)),
            "desktop={desktop}: {}",
            walld.log_contents()
        );
        assert!(!walld.log_contents().contains("skwd-paper-plasma"), "{}", walld.log_contents());
    }
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn plasma_protocol_requires_plugin_despite_non_plasma_desktop_name() {
    let (mut sandbox, _plasma) = plasma_session("plasma-stale-niri");
    sandbox.set_env("XDG_CURRENT_DESKTOP", "niri");
    sandbox.set_env("XDG_DATA_DIRS", &sandbox.root.join("empty").to_string_lossy());
    std::fs::remove_file(
        sandbox.root.join("data/plasma/wallpapers/org.skwd.wall.plasma/metadata.json"),
    )
    .unwrap();
    let image = still(&sandbox, "missing.png", "red");
    let walld = Walld::start(&sandbox);
    let reply =
        walld.client().call("wall.apply", json!({"type": "static", "path": image}), 1).unwrap();
    assert!(err_message(Some(&reply)).contains("skwd-paper-plasma"), "{reply}");
    assert!(child_pids(walld.pid(), STUB).is_empty());
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn plasma_protocol_selects_plugin_without_desktop_hint() {
    let (mut sandbox, plasma) = plasma_session("plasma-no-hint");
    sandbox.set_env("XDG_CURRENT_DESKTOP", "");
    let image = still(&sandbox, "actual.png", "blue");
    let walld = Walld::start(&sandbox);
    call(&walld, &mut walld.client(), "wall.apply", json!({"type": "static", "path": image}));
    assert_eq!(plasma.scripts().len(), 1);
    assert!(child_pids(walld.pid(), STUB).is_empty());
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn explicit_plasma_backend_disable_still_uses_native_renderer() {
    let (mut sandbox, plasma) = plasma_session("plasma-disabled");
    sandbox.set_env("SKWD_PLASMA_BACKEND", "0");
    let image = still(&sandbox, "disabled.png", "blue");
    let walld = Walld::start(&sandbox);
    call(&walld, &mut walld.client(), "wall.apply", json!({"type": "static", "path": image}));
    assert!(wait_until(|| !child_pids(walld.pid(), STUB).is_empty(), Duration::from_secs(5)));
    assert!(plasma.scripts().is_empty());
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn plasma_lock_screen_allows_config_reload_during_assignment() {
    let (mut sandbox, _plasma) = plasma_session("plasma-lock-reload");
    let image = still(&sandbox, "lock.png", "red");
    let gate = sandbox.root.join("config-gate");
    std::fs::create_dir(&gate).unwrap();
    sandbox.set_env("SKWD_E2E_KCONFIG_GATE", gate.to_str().unwrap());
    let mut config: Value =
        serde_json::from_str(&std::fs::read_to_string(sandbox.config_path()).unwrap()).unwrap();
    config["plasma"] = json!({"lockScreen": {"mode": "static", "image": image}});
    sandbox.write_config(&config);
    let lockrc = sandbox.root.join("config/kscreenlockerrc");
    let original = b"[Greeter]\nWallpaperPlugin=org.skwd.wall.plasma\n\n[Greeter][Wallpaper][org.skwd.wall.plasma][General]\nAssignment=previous\n";
    std::fs::write(&lockrc, original).unwrap();
    let walld = Walld::start(&sandbox);
    assert!(wait_until(|| gate.join("entered").exists(), Duration::from_secs(3)));
    let backup = sandbox.root.join("config/kscreenlockerrc.skwd-backup");
    assert_eq!(std::fs::read(&backup).unwrap(), original);
    assert_eq!(std::fs::read(&lockrc).unwrap(), original);
    set_lock_screen_mode(&sandbox, &mut config, "off");
    let mut client = walld.client();
    client.send("effects.list", json!({}), 91);
    std::thread::sleep(Duration::from_millis(500));
    std::fs::write(gate.join("release"), b"").unwrap();
    let response = client.recv(Duration::from_secs(3)).expect("reload completes without deadlock");
    assert_eq!(response["id"], 91);
    assert!(response.get("result").is_some(), "{response}");
    assert!(
        walld.wait_log("synchronized KDE Plasma lock-screen wallpaper", Duration::from_secs(3))
    );
    assert!(wait_until(|| lock_screen_plugin(&sandbox).is_empty(), Duration::from_secs(3)));
    assert_eq!(std::fs::read(backup).unwrap(), original);
    call(&walld, &mut client, "effects.list", json!({}));
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn inactive_activity_is_isolated() {
    let (sandbox, plasma) = plasma_session("plasma-activities");
    let first = still(&sandbox, "a.png", "red");
    let second = still(&sandbox, "b.png", "green");
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let listed = call(&walld, &mut client, "wall.activities", json!({}));
    assert_eq!(listed["result"]["current"], "activity-a");
    assert_eq!(listed["result"]["activities"].as_array().unwrap().len(), 2);
    call(&walld, &mut client, "wall.apply", json!({"type":"static","path":first}));
    let before = call(&walld, &mut client, "wall.outputs", json!({}));
    let current = sandbox.outputs_json();
    let last = sandbox.last_wallpaper();
    let inactive = call(
        &walld,
        &mut client,
        "wall.apply",
        json!({"type":"static","path":second,"output":"DP-1","activity":"activity-b"}),
    );
    assert_eq!(inactive["result"]["visible"], false);
    assert_eq!(sandbox.outputs_json(), current);
    assert_eq!(sandbox.last_wallpaper(), last);
    assert_eq!(inactive["result"]["activity"], "activity-b");
    assert_eq!(before["result"], call(&walld, &mut client, "wall.outputs", json!({}))["result"]);
    let scoped = call(&walld, &mut client, "wall.outputs", json!({"activity":"activity-b"}));
    let rows = scoped["result"]["outputs"].as_array().unwrap();
    assert_eq!(rows.iter().find(|row| row["name"] == "DP-1").unwrap()["path"], second);
    assert_eq!(rows.iter().find(|row| row["name"] == "DP-2").unwrap()["path"], "");
    call(
        &walld,
        &mut client,
        "wall.apply",
        json!({"type":"static","path":first,"output":"DP-2","activity":"activity-b"}),
    );
    let scoped = call(&walld, &mut client, "wall.outputs", json!({"activity":"activity-b"}));
    let rows = scoped["result"]["outputs"].as_array().unwrap();
    assert_eq!(rows.iter().find(|row| row["name"] == "DP-1").unwrap()["path"], second);
    assert_eq!(rows.iter().find(|row| row["name"] == "DP-2").unwrap()["path"], first);
    assert_eq!(
        call(&walld, &mut client, "wall.activities", json!({}))["result"]["current"],
        "activity-a"
    );
    let scripts = plasma.scripts().len();
    for activity in [json!("deleted"), json!(""), json!(42), json!(null)] {
        let response = client
            .call("wall.apply", json!({"type":"static","path":first,"activity":activity}), 9)
            .unwrap();
        assert!(response.get("error").is_some(), "{response}");
    }
    assert_eq!(plasma.scripts().len(), scripts);
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn switched_activity_preserves_outputs() {
    let (sandbox, _plasma) = plasma_session("plasma-switch");
    let first = still(&sandbox, "a.png", "red");
    let second = still(&sandbox, "b.png", "green");
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    call(&walld, &mut client, "wall.apply", json!({"type":"static","path":first}));
    call(
        &walld,
        &mut client,
        "wall.apply",
        json!({"type":"static","path":second,"activity":"activity-b"}),
    );
    std::fs::write(
        sandbox.root.join("plasma/activities.json"),
        json!({"current":"activity-b","names":{"activity-a":"Work","activity-b":"Home"}})
            .to_string(),
    )
    .unwrap();
    call(
        &walld,
        &mut client,
        "wall.apply",
        json!({"type":"static","path":first,"output":"DP-1","activity":"activity-b"}),
    );
    let scoped = call(&walld, &mut client, "wall.outputs", json!({"activity":"activity-b"}));
    let rows = scoped["result"]["outputs"].as_array().unwrap();
    assert_eq!(rows.iter().find(|row| row["name"] == "DP-1").unwrap()["path"], first);
    assert_eq!(rows.iter().find(|row| row["name"] == "DP-2").unwrap()["path"], second);
    let original = call(&walld, &mut client, "wall.outputs", json!({"activity":"activity-a"}));
    assert!(
        original["result"]["outputs"].as_array().unwrap().iter().all(|row| row["path"] == first)
    );
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn restart_uses_current_activity() {
    let (sandbox, _plasma) = plasma_session("activity-restart");
    let first = still(&sandbox, "a.png", "red");
    let second = still(&sandbox, "b.png", "green");
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    call(&walld, &mut client, "wall.apply", json!({"type":"static","path":first}));
    call(
        &walld,
        &mut client,
        "wall.apply",
        json!({"type":"static","path":second,"activity":"activity-b"}),
    );
    drop(client);
    drop(walld);
    std::fs::write(
        sandbox.root.join("plasma/activities.json"),
        json!({"current":"activity-b","names":{"activity-a":"Work","activity-b":"Home"}})
            .to_string(),
    )
    .unwrap();
    let mut config: Value =
        serde_json::from_slice(&std::fs::read(sandbox.config_path()).unwrap()).unwrap();
    config["restoreOnStartup"] = json!(true);
    sandbox.write_config(&config);
    let walld = Walld::start(&sandbox);
    assert!(
        wait_until(|| sandbox.outputs_json()["DP-1"]["path"] == second, Duration::from_secs(5)),
        "{}",
        walld.log_contents()
    );
    let mut client = walld.client();
    let original = call(&walld, &mut client, "wall.outputs", json!({"activity":"activity-a"}));
    assert!(
        original["result"]["outputs"].as_array().unwrap().iter().all(|row| row["path"] == first)
    );
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn activity_waits_for_service() {
    let (sandbox, plasma, image) = restore_session("activity-late-service");
    let prior = plasma.scripts().len();
    let unavailable = sandbox.root.join("plasma/activity-unavailable");
    std::fs::write(&unavailable, "").unwrap();
    let walld = Walld::start(&sandbox);
    assert!(
        walld.wait_log("Plasma Activity restore failed", Duration::from_secs(5)),
        "{}",
        walld.log_contents()
    );
    assert_eq!(plasma.scripts().len(), prior);
    std::fs::remove_file(unavailable).unwrap();
    assert!(
        wait_until(|| plasma.scripts().len() > prior, Duration::from_secs(8)),
        "{}",
        walld.log_contents()
    );
    assert_eq!(source(&scripted(&plasma)["DP-1"]), (SourceKind::Static, image));
    assert!(walld.responsive());
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn assignments_wait_for_data() {
    let (sandbox, plasma, image) = restore_session("plasma-late-assignments");
    let prior = plasma.scripts().len();
    let assignments = sandbox.root.join("plasma/assignments.json");
    let saved = std::fs::read(&assignments).unwrap();
    let unavailable = sandbox.root.join("plasma/assignments-unavailable");
    std::fs::write(&unavailable, "").unwrap();
    let walld = Walld::start(&sandbox);
    assert!(
        walld.wait_log("Plasma Activity restore failed", Duration::from_secs(5)),
        "{}",
        walld.log_contents()
    );
    assert_eq!(plasma.scripts().len(), prior);
    assert_eq!(std::fs::read(&assignments).unwrap(), saved);
    std::fs::remove_file(unavailable).unwrap();
    assert!(
        wait_until(|| plasma.scripts().len() > prior, Duration::from_secs(8)),
        "{}",
        walld.log_contents()
    );
    assert_eq!(source(&scripted(&plasma)["DP-1"]), (SourceKind::Static, image));
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn plasma_waits_for_ready() {
    let (sandbox, plasma, image) = restore_session("plasma-late-ready");
    let prior = plasma.scripts().len();
    plasma.fail_presentations("Plasma starting");
    let walld = Walld::start(&sandbox);
    assert!(
        wait_until(|| plasma.scripts().len() > prior, Duration::from_secs(5)),
        "{}",
        walld.log_contents()
    );
    assert!(
        walld.wait_log("Plasma Activity restore failed", Duration::from_secs(5)),
        "{}",
        walld.log_contents()
    );
    plasma.confirm_presentations();
    assert!(
        wait_until(|| plasma.scripts().len() > prior + 1, Duration::from_secs(8)),
        "{}",
        walld.log_contents()
    );
    assert_eq!(source(&scripted(&plasma)["DP-1"]), (SourceKind::Static, image));
    assert!(walld.responsive());
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn manual_apply_supersedes_restore() {
    let (sandbox, plasma, _image) = restore_session("plasma-manual-supersedes");
    let green = still(&sandbox, "manual.png", "green");
    let prior = plasma.scripts().len();
    plasma.fail_presentations("Plasma starting");
    let walld = Walld::start(&sandbox);
    assert!(
        wait_until(|| plasma.scripts().len() > prior, Duration::from_secs(5)),
        "{}",
        walld.log_contents()
    );
    assert!(
        walld.wait_log("Plasma Activity restore failed", Duration::from_secs(5)),
        "{}",
        walld.log_contents()
    );
    plasma.confirm_presentations();
    let mut client = walld.client();
    call(&walld, &mut client, "wall.apply", json!({"type":"static","path":green}));
    let scripts = plasma.scripts().len();
    assert_eq!(scripts, prior + 2, "{}", walld.log_contents());
    std::thread::sleep(Duration::from_secs(3));
    assert_eq!(plasma.scripts().len(), scripts, "{}", walld.log_contents());
    assert_eq!(source(&scripted(&plasma)["DP-1"]), (SourceKind::Static, green));
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn inactive_apply_keeps_restore() {
    let (sandbox, plasma, image) = restore_session("plasma-inactive-keeps");
    let green = still(&sandbox, "inactive.png", "green");
    let prior = plasma.scripts().len();
    plasma.fail_presentations("Plasma starting");
    let walld = Walld::start(&sandbox);
    assert!(
        wait_until(|| plasma.scripts().len() > prior, Duration::from_secs(5)),
        "{}",
        walld.log_contents()
    );
    assert!(
        walld.wait_log("Plasma Activity restore failed", Duration::from_secs(5)),
        "{}",
        walld.log_contents()
    );
    let mut client = walld.client();
    let reply = call(
        &walld,
        &mut client,
        "wall.apply",
        json!({"type":"static","path":green,"activity":"activity-b"}),
    );
    assert_eq!(reply["result"]["visible"], false);
    let scripts = plasma.scripts().len();
    plasma.confirm_presentations();
    assert!(
        wait_until(|| plasma.scripts().len() > scripts, Duration::from_secs(8)),
        "{}",
        walld.log_contents()
    );
    assert_eq!(source(&scripted(&plasma)["DP-1"]), (SourceKind::Static, image));
    let scoped = call(&walld, &mut client, "wall.outputs", json!({"activity":"activity-b"}));
    assert!(scoped["result"]["outputs"].as_array().unwrap().iter().all(|row| row["path"] == green));
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn native_output_stays_native() {
    let (sandbox, plasma) = plasma_session("activity-native-output");
    let first = still(&sandbox, "a.png", "red");
    let second = still(&sandbox, "b.png", "green");
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    call(&walld, &mut client, "wall.apply", json!({"type":"static","path":first}));
    std::fs::write(
        sandbox.root.join("plasma/native-outputs.json"),
        json!({"activity-a":["DP-2"]}).to_string(),
    )
    .unwrap();
    call(
        &walld,
        &mut client,
        "wall.apply",
        json!({"type":"static","path":second,"activity":"activity-a","output":"DP-1"}),
    );
    let changed = scripted(&plasma);
    assert_eq!(source(&changed["DP-1"]), (SourceKind::Static, second));
    assert!(!changed.contains_key("DP-2"));
    assert!(sandbox.outputs_json().get("DP-2").is_none());
}
