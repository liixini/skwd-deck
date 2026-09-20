use paper_control::{Assignment, SourceKind};
use serde_json::{Map, Value, json};
use skwd_e2e::{
    Client, FakePlasma, FakePlugin, Sandbox, Walld, child_pids, err_message, ffmpeg_still,
    wait_until,
};
use std::time::Duration;

const STUB: &str = "fake_renderer";

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

fn latest(plugin: &FakePlugin) -> Value {
    plugin.entries().pop().unwrap_or(Value::Null)
}

fn delivered(plugin: &FakePlugin, kind: SourceKind, path: &str) -> bool {
    plugin.wait(|lines| {
        lines
            .iter()
            .rev()
            .find_map(|line| line.get("entry"))
            .is_some_and(|entry| source(entry) == (kind, path.to_string()))
    })
}

fn backend(walld: &Walld) -> String {
    walld.client().call("status", json!({}), 2).expect("status")["result"]["playback"]
        ["window_state_backend"]
        .as_str()
        .unwrap_or_default()
        .to_string()
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
fn plasma_pushes_assignments_to_subscribed_outputs_instead_of_scripting() {
    let (sandbox, plasma) = plasma_session("plasma-push");
    let first = still(&sandbox, "a.png", "red");
    let second = still(&sandbox, "b.png", "green");
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let mut left = FakePlugin::connect(&sandbox, "DP-1");
    let mut right = FakePlugin::connect(&sandbox, "DP-2");

    call(&walld, &mut client, "wall.apply", json!({"type": "static", "path": first}));
    assert_eq!(plasma.scripts().len(), 1, "an unsubscribed session is configured by script");

    left.subscribe();
    assert!(delivered(&left, SourceKind::Static, &first), "{:?}", left.lines());
    call(&walld, &mut client, "wall.apply", json!({"type": "static", "path": second}));
    assert_eq!(plasma.scripts().len(), 2, "one unsubscribed output still needs the script");
    assert!(delivered(&left, SourceKind::Static, &second), "{:?}", left.lines());
    assert!(right.entries().is_empty(), "an unsubscribed plugin received {:?}", right.entries());

    right.subscribe();
    assert!(delivered(&right, SourceKind::Static, &second), "{:?}", right.lines());
    call(
        &walld,
        &mut client,
        "wall.apply",
        json!({"type": "static", "path": first, "output": "DP-1"}),
    );
    assert_eq!(plasma.scripts().len(), 2, "{:?}", plasma.calls());
    assert!(delivered(&left, SourceKind::Static, &first), "{:?}", left.lines());
    assert_eq!(source(&latest(&right)), (SourceKind::Static, second.clone()));
    assert_eq!(assignment(&latest(&right)).outputs, ["DP-2"]);
    assert_eq!(latest(&left)["width"], 1920);

    call(&walld, &mut client, "wall.apply", json!({"type": "we", "we_id": "scene-a"}));
    call(
        &walld,
        &mut client,
        "wall.set_audio",
        json!({"mute": false, "volume": 35, "outputs": ["DP-1"]}),
    );
    assert_eq!(plasma.scripts().len(), 2, "{:?}", plasma.calls());
    assert!(
        left.wait(|lines| lines.iter().rev().find_map(|line| line.get("entry")).is_some_and(
            |entry| {
                let assigned = assignment(entry);
                assigned.source.kind == SourceKind::WallpaperEngine
                    && (assigned.mute, assigned.volume) == (false, 35)
            }
        )),
        "{:?}",
        left.lines()
    );
    let untouched = assignment(&latest(&right));
    assert_eq!(untouched.source.kind, SourceKind::WallpaperEngine);
    assert_ne!((untouched.mute, untouched.volume), (false, 35));
    assert!(child_pids(walld.pid(), STUB).is_empty(), "Plasma started a native renderer");
    assert!(!walld.log_contents().contains("panicked"));
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn plasma_plugin_restart_falls_back_to_the_script_and_resumes_the_push() {
    let (sandbox, plasma) = plasma_session("plasma-reconnect");
    let first = still(&sandbox, "a.png", "red");
    let second = still(&sandbox, "b.png", "green");
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();

    call(&walld, &mut client, "wall.apply", json!({"type": "static", "path": first}));
    let mut plugins =
        [FakePlugin::connect(&sandbox, "DP-1"), FakePlugin::connect(&sandbox, "DP-2")];
    for plugin in &mut plugins {
        plugin.subscribe();
        assert!(delivered(plugin, SourceKind::Static, &first), "{:?}", plugin.lines());
    }
    assert!(wait_until(|| backend(&walld) == "plasma", Duration::from_secs(5)));
    call(&walld, &mut client, "wall.apply", json!({"type": "static", "path": second}));
    assert_eq!(plasma.scripts().len(), 1);

    drop(plugins);
    assert!(
        wait_until(|| backend(&walld) != "plasma", Duration::from_secs(5)),
        "{}",
        backend(&walld)
    );
    call(&walld, &mut client, "wall.apply", json!({"type": "static", "path": first}));
    assert_eq!(plasma.scripts().len(), 2, "a restarted shell must be configured by script");
    assert_eq!(source(&scripted(&plasma)["DP-2"]), (SourceKind::Static, first.clone()));

    let mut plugins =
        [FakePlugin::connect(&sandbox, "DP-1"), FakePlugin::connect(&sandbox, "DP-2")];
    for plugin in &mut plugins {
        plugin.subscribe();
        assert!(delivered(plugin, SourceKind::Static, &first), "{:?}", plugin.lines());
        assert_eq!(plugin.entries().len(), 1, "{:?}", plugin.lines());
    }
    call(&walld, &mut client, "wall.apply", json!({"type": "static", "path": second}));
    assert_eq!(plasma.scripts().len(), 2, "{:?}", plasma.calls());
    for plugin in &plugins {
        assert!(delivered(plugin, SourceKind::Static, &second), "{:?}", plugin.lines());
    }
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
    assert!(left.lines().iter().all(|line| line["capabilities"] == json!(["assignments"])));
    left.report(false);
    assert!(paused(&left, false), "{:?}", left.lines());

    call(&walld, &mut client, "wall.apply", json!({"type": "static", "path": image}));
    for plugin in [&mut left, &mut right] {
        plugin.subscribe();
        assert!(delivered(plugin, SourceKind::Static, &image), "{:?}", plugin.lines());
    }
    let scripts = plasma.scripts().len();
    call(&walld, &mut client, "wall.set_paused", json!({"output": "DP-2", "paused": true}));
    assert!(
        right.wait(|lines| lines
            .iter()
            .rev()
            .find_map(|line| line.get("entry"))
            .is_some_and(|entry| entry["manualPaused"] == true && entry["paused"] == true)),
        "{:?}",
        right.lines()
    );
    assert_eq!(latest(&left)["manualPaused"], false, "{:?}", left.lines());
    assert_eq!(plasma.scripts().len(), scripts, "{:?}", plasma.calls());
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
    set_lock_screen_mode(&sandbox, &mut config, "static");
    assert!(selected("org.skwd.wall.plasma"), "no second takeover\n{}", walld.log_contents());
    set_lock_screen_mode(&sandbox, &mut config, "off");
    assert!(selected("org.kde.slideshow"), "user plugin not restored\n{}", walld.log_contents());

    let reloads = plasma
        .calls()
        .iter()
        .filter(|args| args.iter().any(|arg| arg == "org.kde.screensaver.configure"))
        .count();
    assert!(reloads >= 4, "{:?}", plasma.calls());
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
