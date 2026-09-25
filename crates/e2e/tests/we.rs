use serde_json::{Value, json};
use skwd_e2e::{
    Checks, Client, Sandbox, Walld, child_pids, ffmpeg_still, ffmpeg_video, field, wait_until,
    wall_outputs,
};
use std::time::Duration;

const STUB: &str = "fake_renderer";

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn preset_apply_and_property_reset_keep_the_selected_item() {
    let stub = skwd_e2e::stub_renderer!();
    let mut sandbox = Sandbox::new("we-preset");
    scene_dir_with_properties(
        &sandbox,
        "1",
        &json!({"rain":{"type":"bool","value":true,"text":"Rain"}}),
    );
    let preset = sandbox.root.join("we/2");
    std::fs::create_dir(&preset).unwrap();
    std::fs::write(
        preset.join("project.json"),
        r#"{"dependency":"1","preset":{"rain":false},"title":"Subtle"}"#,
    )
    .unwrap();
    sandbox.set_env("SKWD_FAKE_OUTPUTS", "DP-1:1920x1080");
    sandbox.set_env("SKWD_WALL_PAPER_VK", &stub);
    sandbox.write_config(&json!({
        "paths":{"wallpaper":sandbox.library(),"steamWorkshop":sandbox.root.join("we")},
        "restoreOnStartup":false,"general":{"randomInterval":0},
        "effects":{"autoRecolor":false,"autoTheme":""},"transition":{"enabled":false}
    }));
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let call = |client: &mut Client, method: &str, params: Value| {
        let response = client.call(method, params, 900).expect("RPC reply");
        assert!(response.get("error").is_none(), "{response}");
        response
    };
    let declared = rows(Some(&call(&mut client, "wall.we_properties", json!({"we_id":"2"}))));
    assert_eq!(row(&declared, "rain").unwrap()["default"], false);
    call(&mut client, "wall.apply", json!({"type":"we","we_id":"2","output":"DP-1"}));
    assert_eq!(output_id(&mut client, "DP-1"), ("we".into(), "2".into()));
    let written = rows(Some(&call(
        &mut client,
        "wall.set_we_property",
        json!({"we_id":"2","name":"rain","value":true}),
    )));
    assert_eq!(row(&written, "rain").unwrap()["value"], true);
    let reset =
        rows(Some(&call(&mut client, "wall.set_we_property", json!({"we_id":"2","reset":true}))));
    assert_eq!(row(&reset, "rain").unwrap()["value"], false);
    assert_eq!(row(&reset, "rain").unwrap()["overridden"], false);
    let original = rows(Some(&call(&mut client, "wall.we_properties", json!({"we_id":"1"}))));
    assert_eq!(row(&original, "rain").unwrap()["value"], true);
    assert_eq!(output_id(&mut client, "DP-1").1, "2");
}

fn scene_dir(sandbox: &Sandbox, we_id: &str) {
    let dir = sandbox.root.join("we").join(we_id);
    std::fs::create_dir_all(&dir).expect("we scene dir");
    std::fs::write(
        dir.join("project.json"),
        json!({ "type": "scene", "title": we_id }).to_string(),
    )
    .expect("project.json");
    std::fs::write(dir.join("scene.pkg"), b"native scene fixture").expect("scene.pkg");
}

fn output_id(client: &mut Client, name: &str) -> (String, String) {
    wall_outputs(client).into_iter().find(|out| field(out, "name") == name).map_or_else(
        Default::default,
        |out| {
            let ident = if field(&out, "type") == "we" {
                field(&out, "we_id").to_string()
            } else {
                field(&out, "path").rsplit('/').next().unwrap_or("").to_string()
            };
            (field(&out, "type").to_string(), ident)
        },
    )
}

fn all_outputs(client: &mut Client, want: &(String, String)) -> bool {
    ["DP-1", "DP-2", "DP-3"].iter().all(|name| &output_id(client, name) == want)
}

fn reap(walld_pid: u32) {
    for pid in child_pids(walld_pid, STUB) {
        let _ = std::process::Command::new("kill").arg("-9").arg(pid.to_string()).status();
    }
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn we_scene_reconcile() {
    let stub_owned = skwd_e2e::stub_renderer!();
    let stub = stub_owned.as_str();
    let mut sandbox = Sandbox::new("we");
    scene_dir(&sandbox, "scene-a");
    scene_dir(&sandbox, "scene-b");
    let we_root = sandbox.root.join("we").to_string_lossy().into_owned();
    let img = sandbox.library().join("a.png");
    assert!(ffmpeg_still(&img, "color=c=red:s=320x180"), "static fixture");
    let img_str = img.to_string_lossy().into_owned();
    let vid = sandbox.library().join("v.mp4");
    let have_video = ffmpeg_video(&vid, "blue", 1.0);
    let vid_str = vid.to_string_lossy().into_owned();

    sandbox.set_env("SKWD_FAKE_OUTPUTS", "DP-1:1920x1080,DP-2:2560x1440,DP-3:1920x1080");
    sandbox.set_env("SKWD_WALL_PAPER_STILL", stub);
    sandbox.set_env("SKWD_WALL_PAPER_VK", stub);
    sandbox.set_env("SKWD_WALL_LOG", "debug");
    let lib = sandbox.library().to_string_lossy().into_owned();
    sandbox.write_config(&json!({
        "paths": { "wallpaper": lib, "videoWallpaper": lib, "steamWorkshop": we_root, "steamWeAssets": we_root },
        "pickOnlyMode": false,
        "restoreOnStartup": false,
        "general": { "randomInterval": 0 },
        "effects": { "autoRecolor": false, "autoTheme": "" },
        "transition": { "enabled": false },
    }));

    let walld = Walld::start(&sandbox);
    let wpid = walld.pid();
    let mut client = walld.client();
    let mut checks = Checks::default();

    let renderer_count = || child_pids(wpid, STUB).len();

    let apply = |c: &mut Client, id: u64, params: Value| {
        c.call("wall.apply", params, id);
    };
    let we_state = |id: &str| ("we".to_string(), id.to_string());
    let static_state = ("static".to_string(), "a.png".to_string());

    apply(&mut client, 1, json!({ "type": "we", "we_id": "scene-a" }));
    checks.check(
        "WE scene applies to all outputs",
        wait_until(|| all_outputs(&mut client, &we_state("scene-a")), Duration::from_secs(6)),
        String::new,
    );
    checks.check(
        "uniform WE runs exactly one native Vulkan renderer",
        wait_until(|| child_pids(wpid, STUB).len() == 1, Duration::from_secs(6)),
        || format!("{} renderers", renderer_count()),
    );

    apply(&mut client, 2, json!({ "type": "we", "we_id": "scene-b" }));
    checks.check(
        "WE->WE swaps scene on all outputs",
        wait_until(|| all_outputs(&mut client, &we_state("scene-b")), Duration::from_secs(6)),
        String::new,
    );
    checks.check(
        "WE->WE keeps exactly one native Vulkan renderer",
        wait_until(|| child_pids(wpid, STUB).len() == 1, Duration::from_secs(6)),
        || format!("{} renderers", renderer_count()),
    );

    apply(&mut client, 3, json!({ "type": "static", "path": img_str, "output": "*" }));
    checks.check(
        "WE->static clears the scene",
        wait_until(|| all_outputs(&mut client, &static_state), Duration::from_secs(6)),
        String::new,
    );
    checks.check(
        "WE->static replaces the native scene renderer without a leak",
        wait_until(|| child_pids(wpid, STUB).len() == 1, Duration::from_secs(6)),
        || format!("{} renderers", renderer_count()),
    );

    apply(&mut client, 4, json!({ "type": "we", "we_id": "scene-a" }));
    checks.check(
        "static->WE respawns the scene",
        wait_until(|| all_outputs(&mut client, &we_state("scene-a")), Duration::from_secs(6)),
        String::new,
    );

    apply(&mut client, 5, json!({ "type": "static", "path": img_str, "output": "DP-1" }));
    checks.check(
        "per-output: DP-1 diverges to static, DP-2/DP-3 stay WE",
        wait_until(
            || {
                output_id(&mut client, "DP-1") == static_state
                    && output_id(&mut client, "DP-2") == we_state("scene-a")
                    && output_id(&mut client, "DP-3") == we_state("scene-a")
            },
            Duration::from_secs(6),
        ),
        || {
            format!(
                "DP-1={:?} DP-2={:?} DP-3={:?}",
                output_id(&mut client, "DP-1"),
                output_id(&mut client, "DP-2"),
                output_id(&mut client, "DP-3")
            )
        },
    );
    checks.check(
        "per-output WE mix keeps the expected native renderers",
        wait_until(|| child_pids(wpid, STUB).len() == 2, Duration::from_secs(6)),
        || format!("{} renderers for the mixed assignment", renderer_count()),
    );

    apply(&mut client, 6, json!({ "type": "static", "path": img_str, "output": "*" }));
    checks.check(
        "collapse to static removes native scene renderers",
        wait_until(|| child_pids(wpid, STUB).len() == 1, Duration::from_secs(6)),
        || format!("{} renderers", renderer_count()),
    );

    if have_video {
        apply(&mut client, 7, json!({ "type": "we", "we_id": "scene-b" }));
        wait_until(|| child_pids(wpid, STUB).len() == 1, Duration::from_secs(6));
        apply(&mut client, 8, json!({ "type": "video", "path": vid_str, "output": "*" }));
        checks.check(
            "WE->video replaces the native scene renderer without a leak",
            wait_until(|| child_pids(wpid, STUB).len() == 1, Duration::from_secs(6)),
            || format!("{} renderers survived the video apply", renderer_count()),
        );
    }

    checks.check("walld responsive after the WE matrix", walld.responsive(), String::new);
    checks.check("no panics in walld log", !walld.log_contents().contains("panicked"), String::new);

    reap(wpid);
    if checks.failed() {
        sandbox.mark_failed();
    }
    checks.finish();
}

fn scene_dir_with_properties(sandbox: &Sandbox, we_id: &str, properties: &Value) {
    let dir = sandbox.root.join("we").join(we_id);
    std::fs::create_dir_all(&dir).expect("we scene dir");
    std::fs::write(
        dir.join("project.json"),
        json!({ "type": "scene", "title": we_id, "general": { "properties": properties } })
            .to_string(),
    )
    .expect("project.json");
    std::fs::write(dir.join("scene.pkg"), b"native scene fixture").expect("scene.pkg");
}

fn rows(response: Option<&Value>) -> Vec<Value> {
    response
        .and_then(|value| value.get("result")?.get("properties")?.as_array().cloned())
        .unwrap_or_default()
}

fn row<'a>(rows: &'a [Value], name: &str) -> Option<&'a Value> {
    rows.iter().find(|row| row.get("name").and_then(Value::as_str) == Some(name))
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn scene_properties_round_trip() {
    let stub_owned = skwd_e2e::stub_renderer!();
    let stub = stub_owned.as_str();
    let mut sandbox = Sandbox::new("we-properties");
    scene_dir_with_properties(
        &sandbox,
        "scene-p",
        &json!({
            "tint": {"type": "color", "value": "1 1 1", "text": "Tint", "order": 1},
            "glow": {"type": "bool", "value": true, "text": "Glow", "order": 2},
            "zoom": {"min": 0.5, "max": 3.0, "step": 0.01, "value": 1.0, "text": "Zoom", "order": 3}
        }),
    );
    let we_root = sandbox.root.join("we").to_string_lossy().into_owned();
    sandbox.set_env("SKWD_FAKE_OUTPUTS", "DP-1:1920x1080");
    sandbox.set_env("SKWD_WALL_PAPER_STILL", stub);
    sandbox.set_env("SKWD_WALL_PAPER_VK", stub);
    let lib = sandbox.library().to_string_lossy().into_owned();
    sandbox.write_config(&json!({
        "paths": { "wallpaper": lib, "videoWallpaper": lib, "steamWorkshop": we_root, "steamWeAssets": we_root },
        "pickOnlyMode": false,
        "restoreOnStartup": false,
        "general": { "randomInterval": 0 },
        "effects": { "autoRecolor": false, "autoTheme": "" },
        "transition": { "enabled": false },
    }));

    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let mut checks = Checks::default();

    let declared = rows(client.call("wall.we_properties", json!({"we_id": "scene-p"}), 1).as_ref());
    checks.check(
        "declared rows are returned in authored order",
        {
            let names: Vec<&str> =
                declared.iter().filter_map(|row| row.get("name")?.as_str()).collect();
            names == ["tint", "glow", "zoom"]
        },
        || format!("{declared:?}"),
    );
    checks.check(
        "an untyped bounded declaration is reported as a slider with its range",
        row(&declared, "zoom").is_some_and(|zoom| {
            zoom.get("kind").and_then(Value::as_str) == Some("slider")
                && zoom.get("min").and_then(Value::as_f64) == Some(0.5)
                && zoom.get("max").and_then(Value::as_f64) == Some(3.0)
        }),
        || format!("{:?}", row(&declared, "zoom")),
    );
    checks.check(
        "nothing is overridden before a write",
        declared.iter().all(|row| row.get("overridden").and_then(Value::as_bool) == Some(false)),
        || format!("{declared:?}"),
    );

    let written = rows(
        client
            .call(
                "wall.set_we_property",
                json!({"we_id": "scene-p", "name": "zoom", "value": 2.5}),
                2,
            )
            .as_ref(),
    );
    checks.check(
        "a write marks exactly that property overridden and echoes the new value",
        row(&written, "zoom").is_some_and(|zoom| {
            zoom.get("overridden").and_then(Value::as_bool) == Some(true)
                && zoom.get("value").and_then(Value::as_f64) == Some(2.5)
                && zoom.get("default").and_then(Value::as_f64) == Some(1.0)
        }) && row(&written, "tint")
            .is_some_and(|tint| tint.get("overridden").and_then(Value::as_bool) == Some(false)),
        || format!("{written:?}"),
    );

    let reread = rows(client.call("wall.we_properties", json!({"we_id": "scene-p"}), 3).as_ref());
    checks.check(
        "the override survives a fresh read",
        row(&reread, "zoom")
            .is_some_and(|zoom| zoom.get("value").and_then(Value::as_f64) == Some(2.5)),
        || format!("{reread:?}"),
    );

    let other = rows(client.call("wall.we_properties", json!({"we_id": "scene-a"}), 4).as_ref());
    checks
        .check("overrides do not leak to another item", other.is_empty(), || format!("{other:?}"));

    let cleared = rows(
        client.call("wall.set_we_property", json!({"we_id": "scene-p", "reset": true}), 5).as_ref(),
    );
    checks.check(
        "reset returns every property to its authored default",
        cleared.iter().all(|row| row.get("overridden").and_then(Value::as_bool) == Some(false))
            && row(&cleared, "zoom")
                .is_some_and(|zoom| zoom.get("value").and_then(Value::as_f64) == Some(1.0)),
        || format!("{cleared:?}"),
    );

    let invalid = client.call(
        "wall.set_we_property",
        json!({"we_id": "../escape", "name": "x", "value": 1}),
        6,
    );
    checks.check(
        "a traversing id is rejected",
        invalid.and_then(|value| value.get("error").cloned()).is_some(),
        || "expected an error response".to_string(),
    );

    checks.finish();
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn scene_property_swaps_reuse_renderers_and_run_together() {
    let stub = skwd_e2e::stub_renderer!();
    let mut sandbox = Sandbox::new("we-live-props");
    scene_dir_with_properties(
        &sandbox,
        "scene-p",
        &json!({
            "glow": {"type": "bool", "value": true},
            "zoom": {"type": "slider", "min": 0.5, "max": 3.0, "value": 1.0}
        }),
    );
    let trace = sandbox.root.join("swaps.jsonl");
    sandbox.set_env("SKWD_FAKE_OUTPUTS", "DP-1:1920x1080,DP-2:1920x1080,DP-3:1920x1080");
    sandbox.set_env("SKWD_WALL_PAPER_VK", &stub);
    sandbox.set_env("SKWD_FAKE_SWAP_TRACE", &trace.to_string_lossy());
    sandbox.set_env("SKWD_FAKE_SWAP_DELAY_MS", "500");
    sandbox.write_config(&json!({
        "paths": {"wallpaper": sandbox.library(), "steamWorkshop": sandbox.root.join("we")},
        "restoreOnStartup": false,
        "general": {"randomInterval": 0},
        "effects": {"autoRecolor": false, "autoTheme": ""},
        "transition": {"enabled": false},
        "playback": {"fullscreen": true, "fullscreenScope": "display"}
    }));
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let applied = client.call("wall.apply", json!({"type": "we", "we_id": "scene-p"}), 1);
    assert!(applied.as_ref().is_some_and(|reply| reply.get("error").is_none()), "{applied:?}");
    let pids = child_pids(walld.pid(), STUB);
    assert_eq!(pids.len(), 3, "one renderer per output");

    let events = || -> Vec<Value> {
        std::fs::read_to_string(&trace)
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).expect("swap trace"))
            .collect()
    };
    let start = std::time::Instant::now();
    let changed = client.call(
        "wall.set_we_property",
        json!({"we_id": "scene-p", "name": "zoom", "value": 2.5}),
        2,
    );
    eprintln!("three-output property update with 500 ms renderer delay: {:?}", start.elapsed());
    assert_eq!(
        changed.as_ref().and_then(|reply| reply.pointer("/result/reapplied")),
        Some(&json!(true))
    );
    let first = events();
    assert_eq!(first.len(), 6, "{first:?}");
    assert!(first[..3].iter().all(|event| event["event"] == "received"), "{first:?}");
    assert!(first[..3].iter().all(|event| event["command"]["properties"]["zoom"] == 2.5));

    let repeated = client.call(
        "wall.set_we_property",
        json!({"we_id": "scene-p", "name": "zoom", "value": 2.5}),
        3,
    );
    assert_eq!(
        repeated.as_ref().and_then(|reply| reply.pointer("/result/reapplied")),
        Some(&json!(true))
    );
    assert_eq!(events().len(), 6, "acknowledged properties must not rebuild again");

    let newer = client.call(
        "wall.set_we_property",
        json!({"we_id": "scene-p", "name": "glow", "value": false}),
        4,
    );
    assert_eq!(
        newer.as_ref().and_then(|reply| reply.pointer("/result/reapplied")),
        Some(&json!(true))
    );
    let updated = events();
    assert_eq!(updated.len(), 12, "{updated:?}");
    assert!(updated[6..9].iter().all(|event| {
        event["event"] == "received"
            && event["command"]["properties"] == json!({"glow": false, "zoom": 2.5})
    }));

    let reset = client.call("wall.set_we_property", json!({"we_id": "scene-p", "reset": true}), 5);
    assert_eq!(
        reset.as_ref().and_then(|reply| reply.pointer("/result/reapplied")),
        Some(&json!(true))
    );
    let cleared = events();
    assert_eq!(cleared.len(), 18, "{cleared:?}");
    assert!(cleared[12..15].iter().all(|event| {
        event["event"] == "received" && event["command"].get("properties").is_none()
    }));
    let mut current = child_pids(walld.pid(), STUB);
    let mut original = pids;
    current.sort_unstable();
    original.sort_unstable();
    assert_eq!(current, original, "property updates retain the live renderers");
}

fn transition_sandbox(name: &str, configured: bool) -> Sandbox {
    let stub = skwd_e2e::stub_renderer!();
    let mut sandbox = Sandbox::new(name);
    scene_dir(&sandbox, "scene-a");
    scene_dir(&sandbox, "scene-b");
    let video = sandbox.root.join("we/video-a");
    std::fs::create_dir_all(&video).expect("WE video directory");
    std::fs::write(
        video.join("project.json"),
        json!({"type": "video", "file": "clip.mp4"}).to_string(),
    )
    .expect("WE video project");
    assert!(ffmpeg_video(&video.join("clip.mp4"), "blue", 0.5));
    assert!(ffmpeg_still(&sandbox.library().join("before.png"), "color=c=red:s=320x180"));
    sandbox.set_env("SKWD_FAKE_OUTPUTS", "DP-1:1920x1080,DP-2:1920x1080");
    sandbox.set_env("SKWD_WALL_PAPER_STILL", &stub);
    sandbox.set_env("SKWD_WALL_PAPER_VK", &stub);
    sandbox.set_env("SKWD_FAKE_SWAP_TRACE", &sandbox.root.join("swaps.jsonl").to_string_lossy());
    sandbox.write_config(&json!({
        "paths": {
            "wallpaper": sandbox.library(),
            "steamWorkshop": sandbox.root.join("we"),
            "steamWeAssets": sandbox.root.join("we")
        },
        "restoreOnStartup": false,
        "general": {"randomInterval": 0},
        "theme": {"policy": "off"},
        "effects": {"autoRecolor": false, "autoTheme": ""},
        "transition": {"enabled": configured, "shader": "sand-globe", "durationMs": 275}
    }));
    sandbox
}

fn scene_transition_requests() -> [(Value, bool); 3] {
    [
        (
            json!({"transition": true, "transition_shader": "crossfade", "transition_duration_ms": 1234}),
            true,
        ),
        (
            json!({"transition": false, "transition_shader": "crossfade", "transition_duration_ms": 1234}),
            false,
        ),
        (
            json!({"transition": true, "transition_shader": "crossfade", "transition_duration_ms": 1234, "no_transition": true}),
            false,
        ),
    ]
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn we_request_transitions_reach_cold_renderers() {
    for configured in [false, true] {
        let sandbox = transition_sandbox("we-trans-cold", configured);
        let walld = Walld::start(&sandbox);
        let mut client = walld.client();
        for project in ["scene-a", "video-a"] {
            for output in ["*", "DP-1"] {
                for (mut params, fade) in scene_transition_requests() {
                    let reset = client.call(
                    "wall.apply",
                    json!({"type": "static", "path": sandbox.library().join("before.png"), "output": "*", "no_transition": true}),
                    1,
                );
                    assert!(
                        reset.as_ref().is_some_and(|reply| reply.get("result").is_some()),
                        "{reset:?}"
                    );
                    params["type"] = json!("we");
                    params["we_id"] = json!(project);
                    params["output"] = json!(output);
                    let applied = client.call("wall.apply", params.clone(), 2);
                    assert!(
                        applied.as_ref().is_some_and(|reply| reply.get("result").is_some()),
                        "{params:?}: {applied:?}\n{}",
                        walld.log_contents()
                    );
                    let expected = if project == "scene-a" {
                        ("we".into(), "scene-a".into())
                    } else {
                        ("video".into(), "clip.mp4".into())
                    };
                    assert_eq!(output_id(&mut client, "DP-1"), expected);
                    if output == "*" {
                        assert_eq!(output_id(&mut client, "DP-2"), expected);
                    } else {
                        assert_eq!(
                            output_id(&mut client, "DP-2"),
                            ("static".into(), "before.png".into())
                        );
                    }
                    let launches: Vec<Vec<String>> = child_pids(walld.pid(), STUB)
                        .into_iter()
                        .filter_map(|pid| std::fs::read(format!("/proc/{pid}/cmdline")).ok())
                        .map(|bytes| {
                            bytes
                                .split(|byte| *byte == 0)
                                .filter(|arg| !arg.is_empty())
                                .map(|arg| String::from_utf8_lossy(arg).into_owned())
                                .collect::<Vec<_>>()
                        })
                        .filter(|args| {
                            if project == "scene-a" {
                                args.iter().any(|arg| arg == "--scene")
                            } else {
                                args.iter().any(|arg| arg.ends_with("/clip.mp4"))
                            }
                        })
                        .collect();
                    assert_eq!(launches.len(), 1, "{params:?}: {launches:?}");
                    let args = &launches[0];
                    assert_eq!(
                        args.iter().any(|arg| arg == "--transition-from"),
                        fade,
                        "{params:?}: {args:?}"
                    );
                    if fade {
                        assert!(
                            args.windows(2).any(|pair| pair == ["--shader", "crossfade"]),
                            "{args:?}"
                        );
                        assert!(
                            args.windows(2).any(|pair| pair == ["--duration-ms", "1234"]),
                            "{args:?}"
                        );
                        assert!(
                            args.windows(2).any(|pair| pair[0] == "--transition-from"
                                && pair[1].ends_with("/before.png")),
                            "{args:?}"
                        );
                    } else {
                        assert!(
                            !args.iter().any(|arg| arg == "--shader" || arg == "--duration-ms"),
                            "{args:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn scene_request_transitions_reach_warm_swaps() {
    for configured in [false, true] {
        let sandbox = transition_sandbox("we-trans-warm", configured);
        let walld = Walld::start(&sandbox);
        let mut client = walld.client();
        let applied = client.call(
            "wall.apply",
            json!({"type": "we", "we_id": "scene-a", "output": "*", "no_transition": true}),
            1,
        );
        assert!(applied.as_ref().is_some_and(|reply| reply.get("result").is_some()), "{applied:?}");
        let pids = child_pids(walld.pid(), STUB);
        assert_eq!(pids.len(), 1);
        for (index, (mut params, fade)) in scene_transition_requests().into_iter().enumerate() {
            let scene = if index % 2 == 0 { "scene-b" } else { "scene-a" };
            params["type"] = json!("we");
            params["we_id"] = json!(scene);
            params["output"] = json!("*");
            let applied = client.call("wall.apply", params.clone(), 2 + index as u64);
            assert!(
                applied.as_ref().is_some_and(|reply| reply.get("result").is_some()),
                "{params:?}: {applied:?}\n{}",
                walld.log_contents()
            );
            let events: Vec<Value> = std::fs::read_to_string(sandbox.root.join("swaps.jsonl"))
                .expect("swap trace")
                .lines()
                .map(|line| serde_json::from_str::<Value>(line).expect("swap event"))
                .filter(|event| event["event"] == "received")
                .collect();
            assert_eq!(events.len(), index + 1, "{events:?}");
            let command = &events[index]["command"];
            assert!(
                command["to"].as_str().is_some_and(|path| path.ends_with(scene)),
                "{command:?}"
            );
            if fade {
                assert_eq!(command["shader"], "crossfade");
                assert_eq!(command["duration_ms"], 1234);
            } else {
                assert!(command.get("shader").is_none(), "{params:?}: {command:?}");
                assert!(command.get("duration_ms").is_none(), "{params:?}: {command:?}");
            }
            assert_eq!(child_pids(walld.pid(), STUB), pids, "warm swaps retain their renderer");
        }
    }
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn scene_fps_override_persists_and_controls_each_renderer() {
    let stub = skwd_e2e::stub_renderer!();
    let mut sandbox = Sandbox::new("we-fps");
    for id in ["scene-a", "scene-b"] {
        scene_dir(&sandbox, id);
    }
    sandbox.set_env("SKWD_FAKE_OUTPUTS", "DP-1:1920x1080,DP-2:1920x1080");
    sandbox.set_env("SKWD_WALL_PAPER_VK", &stub);
    sandbox.write_config(&json!({
        "paths": {"wallpaper":sandbox.library(), "steamWorkshop":sandbox.root.join("we")},
        "restoreOnStartup":false, "general":{"randomInterval":0},
        "effects":{"autoRecolor":false,"autoTheme":""},
        "transition":{"enabled":false}, "weRender":{"fps":30}
    }));
    let call = |client: &mut Client, method: &str, params: Value| {
        let response = client.call(method, params, 900).expect("RPC reply");
        assert!(response.get("error").is_none(), "{response}");
        response["result"].clone()
    };
    let renderer_fps = |pid: u32, output: &str| -> Option<u32> {
        child_pids(pid, STUB).into_iter().find_map(|child| {
            let args = std::fs::read(format!("/proc/{child}/cmdline")).ok()?;
            if !args.split(|byte| *byte == 0).any(|arg| arg == output.as_bytes()) {
                return None;
            }
            let env = std::fs::read(format!("/proc/{child}/environ")).ok()?;
            env.split(|byte| *byte == 0).find_map(|entry| {
                std::str::from_utf8(entry).ok()?.strip_prefix("SKWD_PAPER_WE_FPS=")?.parse().ok()
            })
        })
    };
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let initial = call(&mut client, "wall.we_properties", json!({"we_id":"scene-a"}));
    assert_eq!(initial["fps"], Value::Null);
    assert_eq!(initial["global_fps"], 30);
    assert_eq!(initial["properties"], json!([]));
    for invalid in [json!(0), json!(241), json!(-1), json!(15.5), json!("15"), json!(true)] {
        let response = client
            .call("wall.set_we_property", json!({"we_id":"scene-a","fps":invalid}), 901)
            .unwrap();
        assert!(response.get("error").is_some(), "{response}");
    }
    let saved = call(&mut client, "wall.set_we_property", json!({"we_id":"scene-a","fps":15}));
    assert_eq!(saved["fps"], 15);
    assert_eq!(saved["reapplied"], false);
    call(&mut client, "wall.apply", json!({"type":"we","we_id":"scene-a","output":"DP-1"}));
    call(&mut client, "wall.apply", json!({"type":"we","we_id":"scene-b","output":"DP-2"}));
    assert_eq!(renderer_fps(walld.pid(), "DP-1"), Some(15));
    assert_eq!(renderer_fps(walld.pid(), "DP-2"), Some(30));
    let changed = call(&mut client, "wall.set_we_property", json!({"we_id":"scene-a","fps":24}));
    assert_eq!(changed["reapplied"], true);
    assert_eq!(renderer_fps(walld.pid(), "DP-1"), Some(24));
    assert_eq!(renderer_fps(walld.pid(), "DP-2"), Some(30));
    assert_eq!(output_id(&mut client, "DP-1").1, "scene-a");
    assert_eq!(output_id(&mut client, "DP-2").1, "scene-b");
    drop(client);
    drop(walld);
    let mut config: Value =
        serde_json::from_slice(&std::fs::read(sandbox.config_path()).unwrap()).unwrap();
    config["weRender"]["fps"] = json!(20);
    sandbox.write_config(&config);
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let persisted = call(&mut client, "wall.we_properties", json!({"we_id":"scene-a"}));
    assert_eq!(persisted["fps"], 24);
    assert_eq!(persisted["global_fps"], 20);
    call(&mut client, "wall.apply", json!({"type":"we","we_id":"scene-a","output":"*"}));
    let cleared = call(&mut client, "wall.set_we_property", json!({"we_id":"scene-a","fps":null}));
    assert_eq!(cleared["fps"], Value::Null);
    assert_eq!(cleared["global_fps"], 20);
    let pids = child_pids(walld.pid(), STUB);
    assert!(!pids.is_empty());
    for pid in pids {
        let env = std::fs::read(format!("/proc/{pid}/environ")).unwrap();
        assert!(env.split(|byte| *byte == 0).any(|entry| entry == b"SKWD_PAPER_WE_FPS=20"));
    }
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn we_external_handoff_survives_picker_close_and_returns_to_paper() {
    let stub = skwd_e2e::stub_renderer!();
    let mut sandbox = Sandbox::new("we-external-handoff");
    sandbox.set_env(
        "SKWD_PAPER_V2_SOCKET",
        &sandbox.root.join("runtime/paper.sock").to_string_lossy(),
    );
    scene_dir(&sandbox, "123");
    sandbox.set_env("SKWD_FAKE_OUTPUTS", "DP-1:1920x1080,DP-2:1920x1080");
    sandbox.set_env("SKWD_WALL_PAPER_STILL", &stub);
    sandbox.set_env("SKWD_WALL_PAPER_VK", &stub);
    let image = sandbox.library().join("image.png");
    assert!(ffmpeg_still(&image, "color=c=red:s=32x32"));
    let marker = sandbox.root.join("external-applied");
    let mut config = json!({
        "paths":{"wallpaper":sandbox.library(), "steamWorkshop":sandbox.root.join("we")},
        "pickOnlyMode":false, "restoreOnStartup":false,
        "transition":{"enabled":false}, "general":{"randomInterval":0},
        "theme":{"policy":"off"}
    });
    sandbox.write_config(&config);
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let response = client.call("wall.apply", json!({"type":"we", "we_id":"123"}), 1).unwrap();
    assert!(response.get("error").is_none(), "{response}");
    assert!(wait_until(|| !child_pids(walld.pid(), STUB).is_empty(), Duration::from_secs(5)));
    let old = child_pids(walld.pid(), STUB);
    let check_old =
        old.iter().map(|pid| format!("test ! -d /proc/{pid}")).collect::<Vec<_>>().join(" && ");
    config["pickOnlyMode"] = json!(true);
    config["postProcessing"] = json!([{"type":"static", "command":format!("{check_old} && printf applied > '{}'", marker.display())}]);
    sandbox.write_config(&config);
    let response = client.call("wall.apply", json!({"type":"static", "path":image}), 2).unwrap();
    assert!(response.get("error").is_none(), "{response}");
    assert!(
        wait_until(|| marker.exists(), Duration::from_secs(5)),
        "external hook must run after Paper exits"
    );
    client.call("picker.session.end", json!({}), 3);
    drop(client);
    let mut client = walld.client();
    client.call("wall.set_paused", json!({"paused":false}), 4);
    assert!(child_pids(walld.pid(), STUB).is_empty());
    assert_eq!(sandbox.outputs_json(), json!({}));
    assert_eq!(sandbox.last_wallpaper()["path"], json!(image));
    config["pickOnlyMode"] = json!(false);
    config["postProcessing"] = json!([]);
    sandbox.write_config(&config);
    let response = client.call("wall.apply", json!({"type":"we", "we_id":"123"}), 5).unwrap();
    assert!(response.get("error").is_none(), "{response}");
    assert!(wait_until(|| !child_pids(walld.pid(), STUB).is_empty(), Duration::from_secs(5)));
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn we_external_handoff_preserves_locked_display() {
    let stub = skwd_e2e::stub_renderer!();
    let mut sandbox = Sandbox::new("we-external-locked");
    sandbox.set_env(
        "SKWD_PAPER_V2_SOCKET",
        &sandbox.root.join("runtime/paper.sock").to_string_lossy(),
    );
    scene_dir(&sandbox, "123");
    sandbox.set_env("SKWD_FAKE_OUTPUTS", "DP-1:1920x1080,DP-2:1920x1080,DP-3:1920x1080");
    sandbox.set_env("SKWD_WALL_PAPER_STILL", &stub);
    sandbox.set_env("SKWD_WALL_PAPER_VK", &stub);
    let image = sandbox.library().join("image.png");
    assert!(ffmpeg_still(&image, "color=c=red:s=32x32"));
    let mut config = json!({
        "paths":{"wallpaper":sandbox.library(), "steamWorkshop":sandbox.root.join("we")},
        "pickOnlyMode":false, "restoreOnStartup":false,
        "transition":{"enabled":false}, "general":{"randomInterval":0},
        "theme":{"policy":"off"}
    });
    sandbox.write_config(&config);
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let response = client.call("wall.apply", json!({"type":"we", "we_id":"123"}), 1).unwrap();
    assert!(response.get("error").is_none(), "{response}");
    config["pickOnlyMode"] = json!(true);
    config["display"] = json!({"outputLocks":{"DP-1":true}});
    sandbox.write_config(&config);
    let response = client.call("wall.apply", json!({"type":"static", "path":image}), 2).unwrap();
    assert!(response.get("error").is_none(), "{response}");
    assert_eq!(response["result"]["locked"], json!(["DP-1"]));
    assert_eq!(sandbox.outputs_json()["DP-1"]["we_id"], "123");
    assert!(sandbox.outputs_json().get("DP-2").is_none());
    assert!(sandbox.outputs_json().get("DP-3").is_none());
    assert!(wait_until(|| child_pids(walld.pid(), STUB).len() == 1, Duration::from_secs(5)));
    client.call("picker.session.end", json!({}), 3);
    assert_eq!(sandbox.outputs_json()["DP-1"]["we_id"], "123");
}
