use serde_json::{Value, json};
use skwd_e2e::{
    Checks, Client, Sandbox, Walld, child_pids, ffmpeg_still, ffmpeg_video, field, wait_until,
    wall_outputs,
};
use std::time::Duration;

const STUB: &str = "fake_renderer";

fn stub_count(walld_pid: u32) -> usize {
    child_pids(walld_pid, STUB).len()
}

fn reap_stubs(walld_pid: u32) {
    for pid in child_pids(walld_pid, STUB) {
        let _ = std::process::Command::new("kill").arg("-9").arg(pid.to_string()).status();
    }
}

fn star(sandbox: &Sandbox, field: &str) -> Value {
    sandbox
        .outputs_json()
        .get("*")
        .and_then(|entry| entry.get(field))
        .cloned()
        .unwrap_or(Value::Null)
}

fn wait_star(sandbox: &Sandbox, field: &str, want: &Value) -> bool {
    wait_until(|| &star(sandbox, field) == want, Duration::from_secs(5))
}

fn apply(client: &mut Client, id: u64, params: Value) {
    client.call("wall.apply", params, id);
}

fn output_path(client: &mut Client, name: &str) -> String {
    wall_outputs(client)
        .into_iter()
        .find(|output| field(output, "name") == name)
        .map_or_else(String::new, |output| field(&output, "path").to_string())
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn apply_state_headless() {
    let stub_owned = skwd_e2e::stub_renderer!();
    let stub = stub_owned.as_str();
    let mut sandbox = Sandbox::new("apply");
    let lib = sandbox.library();
    let img = lib.join("a.png");
    let img2 = lib.join("b.png");
    assert!(ffmpeg_still(&img, "color=c=red:s=320x180"), "ffmpeg fixture a");
    assert!(ffmpeg_still(&img2, "color=c=green:s=320x180"), "ffmpeg fixture b");
    let vid = lib.join("v.mp4");
    let have_video = ffmpeg_video(&vid, "blue", 1.0);

    sandbox.set_env("SKWD_WALL_PAPER_STILL", stub);
    sandbox.set_env("SKWD_WALL_PAPER_VK", stub);
    let lib_str = lib.to_string_lossy().into_owned();
    sandbox.write_config(&json!({
        "paths": { "wallpaper": lib_str, "videoWallpaper": lib_str },
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
    let img_str = img.to_string_lossy().into_owned();
    let img2_str = img2.to_string_lossy().into_owned();

    apply(&mut client, 1, json!({ "type": "static", "path": img_str }));
    checks.check(
        "static apply spawns exactly one renderer",
        wait_until(|| stub_count(wpid) == 1, Duration::from_secs(6)),
        || format!("stub count = {}", stub_count(wpid)),
    );
    checks.check(
        "static apply records type=static in outputs.json",
        wait_star(&sandbox, "type", &json!("static")) && star(&sandbox, "path") == json!(img_str),
        || format!("{}", sandbox.outputs_json()),
    );
    checks.check(
        "static apply records last-wallpaper.json",
        sandbox.last_wallpaper().get("type") == Some(&json!("static")),
        || format!("{}", sandbox.last_wallpaper()),
    );

    apply(&mut client, 2, json!({ "type": "static", "path": img2_str }));
    checks.check(
        "static->static replaces, exactly one renderer (no leak)",
        wait_until(|| stub_count(wpid) == 1, Duration::from_secs(6)),
        || format!("stub count = {}", stub_count(wpid)),
    );
    checks.check(
        "static->static updates the path in outputs.json",
        wait_star(&sandbox, "path", &json!(img2_str)),
        || format!("{}", sandbox.outputs_json()),
    );

    if have_video {
        let vid_str = vid.to_string_lossy().into_owned();
        apply(
            &mut client,
            3,
            json!({ "type": "video", "path": vid_str, "mute": false, "volume": 70 }),
        );
        checks.check(
            "static->video replaces, exactly one renderer",
            wait_until(|| stub_count(wpid) == 1, Duration::from_secs(6)),
            || format!("stub count = {}", stub_count(wpid)),
        );
        checks.check(
            "video apply records type=video",
            wait_star(&sandbox, "type", &json!("video"))
                && star(&sandbox, "path") == json!(vid_str),
            || format!("{}", sandbox.outputs_json()),
        );
        checks.check(
            "video apply records unmuted audio + volume",
            wait_star(&sandbox, "mute", &json!(false)) && star(&sandbox, "volume") == json!(70),
            || format!("{}", sandbox.outputs_json()),
        );

        client.call("wall.set_audio", json!({ "mute": true }), 4);
        checks.check(
            "set_audio mutes the entry in outputs.json",
            wait_star(&sandbox, "mute", &json!(true)),
            || format!("{}", sandbox.outputs_json()),
        );
        client.call("wall.set_audio", json!({ "volume": 30 }), 5);
        checks.check(
            "set_audio updates volume in outputs.json",
            wait_star(&sandbox, "volume", &json!(30)),
            || format!("{}", sandbox.outputs_json()),
        );

        apply(&mut client, 6, json!({ "type": "static", "path": img_str }));
        checks.check(
            "video->static replaces, exactly one renderer (no video leak)",
            wait_until(|| stub_count(wpid) == 1, Duration::from_secs(6)),
            || format!("stub count = {}", stub_count(wpid)),
        );
        checks.check(
            "video->static clears the video entry (type back to static)",
            wait_star(&sandbox, "type", &json!("static")),
            || format!("{}", sandbox.outputs_json()),
        );
    } else {
        eprintln!("  note  no video encoder; video + audio legs skipped");
    }

    checks.check("walld responsive after the apply matrix", walld.responsive(), String::new);
    checks.check("no panics in walld log", !walld.log_contents().contains("panicked"), String::new);

    reap_stubs(wpid);
    if checks.failed() {
        sandbox.mark_failed();
    }
    checks.finish();
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn output_lock_blocks_apply() {
    let stub_owned = skwd_e2e::stub_renderer!();
    let stub = stub_owned.as_str();
    let mut sandbox = Sandbox::new("apply-locks");
    let lib = sandbox.library();
    let red = lib.join("red.png");
    let green = lib.join("green.png");
    assert!(ffmpeg_still(&red, "color=c=red:s=320x180"), "red fixture");
    assert!(ffmpeg_still(&green, "color=c=green:s=320x180"), "green fixture");

    sandbox.set_env("SKWD_FAKE_OUTPUTS", "DP-1:1920x1080,DP-2:2560x1440");
    sandbox.set_env("SKWD_WALL_PAPER_STILL", stub);
    sandbox.set_env("SKWD_WALL_PAPER_VK", stub);
    let lib_str = lib.to_string_lossy().into_owned();
    sandbox.write_config(&json!({
        "paths": { "wallpaper": lib_str, "videoWallpaper": lib_str },
        "pickOnlyMode": false,
        "restoreOnStartup": false,
        "general": { "randomInterval": 0 },
        "effects": { "autoRecolor": false, "autoTheme": "" },
        "transition": { "enabled": false },
        "display": { "outputLocks": { "DP-1": true } },
    }));

    let walld = Walld::start(&sandbox);
    let wpid = walld.pid();
    let mut client = walld.client();
    let mut checks = Checks::default();
    let red_str = red.to_string_lossy().into_owned();
    let green_str = green.to_string_lossy().into_owned();

    let response =
        client.call("wall.apply", json!({ "type": "static", "path": red_str, "output": "*" }), 1);
    checks.check(
        "manual global apply reports locked and applied outputs",
        response.as_ref().is_some_and(|value| {
            value["result"]["locked"] == json!(["DP-1"])
                && value["result"]["applied"] == json!(["DP-2"])
        }),
        || format!("{response:?}"),
    );
    checks.check(
        "manual global apply leaves the locked output unchanged",
        wait_until(
            || {
                output_path(&mut client, "DP-1").is_empty()
                    && output_path(&mut client, "DP-2") == red_str
            },
            Duration::from_secs(8),
        ),
        || format!("{:?}", wall_outputs(&mut client)),
    );

    let locked = client.call(
        "wall.apply",
        json!({ "type": "static", "path": green_str, "output": "DP-1" }),
        2,
    );
    checks.check(
        "manual per-output apply is rejected while locked",
        locked.as_ref().and_then(|value| value.get("result")?.get("locked"))
            == Some(&json!("DP-1"))
            && output_path(&mut client, "DP-1").is_empty(),
        || format!("response={locked:?} outputs={:?}", wall_outputs(&mut client)),
    );

    let overridden = client.call(
        "wall.apply",
        json!({
            "type": "static", "path": green_str, "output": "DP-1",
            "override_locks": true
        }),
        3,
    );
    checks.check(
        "picker override applies while preserving the output lock",
        overridden.as_ref().and_then(|value| value.get("result")?.get("applied"))
            == Some(&json!(green_str))
            && wait_until(|| output_path(&mut client, "DP-1") == green_str, Duration::from_secs(8)),
        || format!("response={overridden:?} outputs={:?}", wall_outputs(&mut client)),
    );
    let still_locked = client.call(
        "wall.apply",
        json!({ "type": "static", "path": red_str, "output": "DP-1" }),
        4,
    );
    checks.check(
        "output remains locked after the picker override",
        still_locked.as_ref().and_then(|value| value.get("result")?.get("locked"))
            == Some(&json!("DP-1"))
            && output_path(&mut client, "DP-1") == green_str,
        || format!("response={still_locked:?} outputs={:?}", wall_outputs(&mut client)),
    );

    let unlocked = json!({
        "paths": { "wallpaper": lib_str, "videoWallpaper": lib_str },
        "pickOnlyMode": false,
        "restoreOnStartup": false,
        "general": { "randomInterval": 0 },
        "effects": { "autoRecolor": false, "autoTheme": "" },
        "transition": { "enabled": false },
        "display": { "outputLocks": { "DP-1": false } },
    });
    sandbox.write_config(&unlocked);
    std::thread::sleep(Duration::from_millis(500));
    apply(&mut client, 5, json!({ "type": "static", "path": red_str, "output": "DP-1" }));
    checks.check(
        "manual apply succeeds after explicit unlock",
        wait_until(|| output_path(&mut client, "DP-1") == red_str, Duration::from_secs(8)),
        || format!("{:?}", wall_outputs(&mut client)),
    );
    checks.check("walld remains responsive after locked applies", walld.responsive(), String::new);
    checks.check("no panics in walld log", !walld.log_contents().contains("panicked"), String::new);

    reap_stubs(wpid);
    if checks.failed() {
        sandbox.mark_failed();
    }
    checks.finish();
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn still_on_one_output_keeps_its_audio_memory() {
    let stub_owned = skwd_e2e::stub_renderer!();
    let stub = stub_owned.as_str();
    let mut sandbox = Sandbox::new("apply-still-audio");
    let lib = sandbox.library();
    let img = lib.join("a.png");
    let vid = lib.join("v.mp4");
    let other = lib.join("w.mp4");
    assert!(ffmpeg_still(&img, "color=c=red:s=320x180"), "ffmpeg fixture a");
    assert!(ffmpeg_video(&vid, "blue", 1.0), "ffmpeg fixture v");
    assert!(ffmpeg_video(&other, "yellow", 1.0), "ffmpeg fixture w");

    sandbox.set_env("SKWD_FAKE_OUTPUTS", "DP-1:1920x1080,DP-2:2560x1440");
    sandbox.set_env("SKWD_WALL_PAPER_STILL", stub);
    sandbox.set_env("SKWD_WALL_PAPER_VK", stub);
    let lib_str = lib.to_string_lossy().into_owned();
    sandbox.write_config(&json!({
        "paths": { "wallpaper": lib_str, "videoWallpaper": lib_str },
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
    let audio = |output: &str| {
        let entry = sandbox.outputs_json()[output].clone();
        (entry["type"].clone(), entry["mute"].clone(), entry["volume"].clone())
    };

    apply(&mut client, 1, json!({ "type": "video", "path": vid, "mute": false, "volume": 70 }));
    checks.check(
        "video apply records unmuted audio at volume 70",
        wait_star(&sandbox, "volume", &json!(70)) && star(&sandbox, "mute") == json!(false),
        || format!("{}", sandbox.outputs_json()),
    );

    apply(&mut client, 2, json!({ "type": "static", "path": img, "output": "DP-1" }));
    checks.check(
        "a still on one output keeps that output's audio memory",
        wait_until(
            || audio("DP-1") == (json!("static"), json!(false), json!(70)),
            Duration::from_secs(6),
        ),
        || format!("{}", sandbox.outputs_json()),
    );

    apply(&mut client, 3, json!({ "type": "video", "path": other, "output": "DP-1" }));
    checks.check(
        "a video back on that output plays at the remembered volume",
        wait_until(
            || audio("DP-1") == (json!("video"), json!(false), json!(70)),
            Duration::from_secs(6),
        ),
        || format!("{}", sandbox.outputs_json()),
    );
    checks.check("no panics in walld log", !walld.log_contents().contains("panicked"), String::new);

    reap_stubs(wpid);
    if checks.failed() {
        sandbox.mark_failed();
    }
    checks.finish();
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn saved_scheme_reaches_current_theme_and_templates() {
    use skwd_wall_core::material;
    use std::fmt::Write;

    let stub = skwd_e2e::stub_renderer!();
    let mut sandbox = Sandbox::new("full-scheme");
    sandbox.set_env("SKWD_WALL_PAPER_STILL", &stub);
    let image = sandbox.library().join("theme.png");
    assert!(ffmpeg_still(&image, "color=c=red:s=320x180"));
    let mut scheme = material::document("#84d1ce", true).unwrap();
    for (index, key) in material::ROLE_KEYS.iter().enumerate() {
        for (mode, base) in [("dark", 0x0012_3400), ("light", 0x00ab_cd00)] {
            scheme["colors"][key][mode]["color"] = json!(format!("#{:06x}", base + index));
        }
    }
    material::select_mode(&mut scheme, true);
    let mut palette = material::ui_palette(&scheme).unwrap();
    palette["name"] = json!("Edited");
    palette["_scheme"] = scheme.clone();
    palette["_schemeVersion"] = json!(1);
    let template = sandbox.root.join("roles.txt");
    let output = sandbox.root.join("roles.out");
    let mut text = String::new();
    for key in material::ROLE_KEYS {
        writeln!(text, "{{{{colors.{key}.default.hex}}}}").unwrap();
    }
    std::fs::write(&template, text).unwrap();
    let mut config = json!({
        "paths": {"wallpaper": sandbox.library()},
        "pickOnlyMode": false, "restoreOnStartup": false,
        "general": {"randomInterval": 0}, "transition": {"enabled": false},
        "theme": {"policy": "fixed", "mode": "dark", "staticTheme": "Edited", "savedThemes": [palette.clone()]},
        "integrations": [{"template": template, "output": output}]
    });
    sandbox.write_config(&config);
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let applied = client.call("wall.apply", json!({"type": "static", "path": image}), 1).unwrap();
    assert!(applied.get("error").is_none(), "{applied}");
    for (iteration, mode) in ["dark", "light"].into_iter().enumerate() {
        let mut expected = String::new();
        for key in material::ROLE_KEYS {
            writeln!(expected, "{}", material::role(&scheme, key, mode).unwrap()).unwrap();
        }
        assert!(
            wait_until(
                || std::fs::read_to_string(&output).is_ok_and(|text| text == expected),
                Duration::from_secs(10)
            ),
            "{mode} template output"
        );
        let mut current = Value::Null;
        assert!(
            wait_until(
                || {
                    current = client
                        .call("theme.current", json!({}), 2 + iteration as u64)
                        .and_then(|response| response.get("result").cloned())
                        .unwrap_or(Value::Null);
                    current["scheme"]["mode"] == mode
                        && current["dark"] == (mode == "dark")
                        && material::ROLE_KEYS.iter().all(|key| {
                            current["scheme"]["colors"][key]["default"]
                                == scheme["colors"][key][mode]
                        })
                },
                Duration::from_secs(10)
            ),
            "{mode} applied scheme: {current}"
        );
        if iteration == 0 {
            config["theme"]["policy"] = json!("wallpaper");
            config["theme"]["mode"] = json!("light");
            config["theme"]["wallpaperProfiles"] =
                json!([{"key": current["key"], "enabled": true, "light": palette}]);
            sandbox.write_config(&config);
            let response = client.call("wall.retheme", json!({}), 4).unwrap();
            assert!(response.get("error").is_none(), "{response}");
        }
    }
    drop(walld);
}
