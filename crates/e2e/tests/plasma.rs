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
    let plasma = FakePlasma::install(&mut sandbox, env!("CARGO_BIN_EXE_fake_qdbus"));
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
