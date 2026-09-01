use serde_json::{Value, json};
use skwd_e2e::{
    Checks, Client, Sandbox, Walld, child_pids, ffmpeg_still, ffmpeg_video, field, wait_until,
    wall_outputs,
};
use std::time::Duration;

const STUB: &str = "fake_renderer";

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
