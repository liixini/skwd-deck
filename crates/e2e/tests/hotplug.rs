use serde_json::{Value, json};
use skwd_e2e::{
    Checks, Client, Sandbox, Walld, child_pids, ffmpeg_still, ffmpeg_video, wait_until,
    wall_outputs,
};
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

const STUB: &str = "fake_renderer";

fn set_monitors(file: &Path, specs: &[&str]) {
    std::fs::write(file, specs.join(",")).expect("write outputs file");
}

fn names(client: &mut Client) -> Vec<String> {
    wall_outputs(client)
        .iter()
        .map(|out| out.get("name").and_then(Value::as_str).unwrap_or("").to_string())
        .collect()
}

fn connected_names(client: &mut Client) -> Vec<String> {
    wall_outputs(client)
        .iter()
        .filter(|out| out.get("connected").and_then(Value::as_bool).unwrap_or(true))
        .map(|out| out.get("name").and_then(Value::as_str).unwrap_or("").to_string())
        .collect()
}

fn target(client: &mut Client, name: &str) -> String {
    wall_outputs(client)
        .iter()
        .find(|out| out.get("name").and_then(Value::as_str) == Some(name))
        .and_then(|out| out.get("target"))
        .and_then(Value::as_str)
        .unwrap_or(name)
        .to_string()
}

fn path(client: &mut Client, name: &str) -> String {
    wall_outputs(client)
        .iter()
        .find(|out| out.get("name").and_then(Value::as_str) == Some(name))
        .and_then(|out| out.get("path"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn has_wallpaper(client: &mut Client, name: &str) -> bool {
    wall_outputs(client)
        .iter()
        .find(|out| out.get("name").and_then(Value::as_str) == Some(name))
        .is_some_and(|out| {
            let ty = out.get("type").and_then(Value::as_str).unwrap_or("");
            let path = out.get("path").and_then(Value::as_str).unwrap_or("");
            !ty.is_empty() && !path.is_empty()
        })
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn hotplug_add_remove_reconciles() {
    let stub_owned = skwd_e2e::stub_renderer!();
    let stub = stub_owned.as_str();
    let mut sandbox = Sandbox::new("hotplug");
    let lib = sandbox.library();
    let img = lib.join("a.png");
    assert!(ffmpeg_still(&img, "color=c=red:s=320x180"), "static fixture");
    let img_str = img.to_string_lossy().into_owned();
    let replacement = lib.join("offline.png");
    assert!(ffmpeg_still(&replacement, "color=c=green:s=320x180"), "offline fixture");
    let replacement_str = replacement.to_string_lossy().into_owned();
    let vid = lib.join("v.mp4");
    let have_video = ffmpeg_video(&vid, "blue", 1.0);
    let vid_str = vid.to_string_lossy().into_owned();

    let outputs_file = sandbox.root.join("fake-outputs.txt");
    set_monitors(&outputs_file, &["DP-1:1920x1080", "DP-2:2560x1440", "DP-3:1920x1080"]);

    sandbox.set_env("SKWD_FAKE_OUTPUTS_FILE", &outputs_file.to_string_lossy());
    sandbox.set_env("SKWD_WALL_PAPER_STILL", stub);
    sandbox.set_env("SKWD_WALL_PAPER_VK", stub);
    sandbox.set_env("SKWD_WALL_LOG", "debug");
    let lib_str = lib.to_string_lossy().into_owned();
    sandbox.write_config(&json!({
        "paths": { "wallpaper": lib_str, "videoWallpaper": lib_str },
        "pickOnlyMode": false,
        "restoreOnStartup": false,
        "general": { "randomInterval": 0 },
        "display": { "outputLocks": { "DP-3": true } },
        "effects": { "autoRecolor": false, "autoTheme": "" },
        "transition": { "enabled": false },
    }));

    let walld = Walld::start(&sandbox);
    let wpid = walld.pid();
    let mut client = walld.client();
    let mut checks = Checks::default();
    let count = || child_pids(wpid, STUB).len();

    let divergent = if have_video {
        client.call(
            "wall.apply",
            json!({ "type": "video", "path": vid_str, "output": "*", "override_locks": true }),
            1,
        );
        client.call(
            "wall.apply",
            json!({ "type": "static", "path": img_str, "output": "DP-1", "override_locks": true }),
            2,
        );
        true
    } else {
        client.call(
            "wall.apply",
            json!({ "type": "static", "path": img_str, "output": "*", "override_locks": true }),
            1,
        );
        client.call(
            "wall.apply",
            json!({ "type": "static", "path": img_str, "output": "DP-1", "override_locks": true }),
            2,
        );
        false
    };
    let _ = divergent;
    checks.check(
        "initial 3-monitor state is live",
        wait_until(
            || names(&mut client).len() == 3 && has_wallpaper(&mut client, "DP-3"),
            Duration::from_secs(6),
        ),
        || format!("{:?}", names(&mut client)),
    );
    let before_remove: HashSet<u32> = child_pids(wpid, STUB).into_iter().collect();

    set_monitors(&outputs_file, &["DP-1:1920x1080", "DP-2:2560x1440"]);
    checks.check(
        "hotplug remove: DP-3 remains manageable but is offline",
        wait_until(
            || {
                names(&mut client).contains(&"DP-3".to_string())
                    && !connected_names(&mut client).contains(&"DP-3".to_string())
            },
            Duration::from_secs(8),
        ),
        || format!("all={:?}, connected={:?}", names(&mut client), connected_names(&mut client)),
    );
    checks.check(
        "hotplug remove: DP-3's renderer is reaped (no leak)",
        wait_until(
            || {
                let current: HashSet<u32> = child_pids(wpid, STUB).into_iter().collect();
                current.len() <= 2 && current != before_remove
            },
            Duration::from_secs(8),
        ),
        || format!("pids={:?}, before={before_remove:?}", child_pids(wpid, STUB)),
    );
    checks.check(
        "DP-1/DP-2 still have wallpapers after remove",
        has_wallpaper(&mut client, "DP-1") && has_wallpaper(&mut client, "DP-2"),
        String::new,
    );
    let after_remove: HashSet<u32> = child_pids(wpid, STUB).into_iter().collect();
    let offline_target = target(&mut client, "DP-3");
    client.call(
        "wall.apply",
        json!({
            "type": "static",
            "path": replacement_str,
            "output": offline_target,
            "override_locks": true
        }),
        3,
    );
    checks.check(
        "offline DP-3 accepts a replacement wallpaper without spawning a renderer",
        wait_until(
            || path(&mut client, "DP-3") == replacement_str && child_pids(wpid, STUB).len() <= 2,
            Duration::from_secs(4),
        ),
        || format!("outputs={:?}, pids={:?}", wall_outputs(&mut client), child_pids(wpid, STUB)),
    );

    set_monitors(&outputs_file, &["DP-1:1920x1080", "DP-2:2560x1440", "DP-3:1920x1080"]);
    checks.check(
        "hotplug add: DP-3 reappears",
        wait_until(
            || connected_names(&mut client).contains(&"DP-3".to_string()),
            Duration::from_secs(8),
        ),
        || format!("{:?}", connected_names(&mut client)),
    );
    checks.check(
        "hotplug add: DP-3 gets a wallpaper",
        wait_until(
            || has_wallpaper(&mut client, "DP-3") && path(&mut client, "DP-3") == replacement_str,
            Duration::from_secs(8),
        ),
        || format!("{:?}", wall_outputs(&mut client)),
    );
    checks.check(
        "hotplug add: DP-3 gets a replacement renderer",
        wait_until(
            || child_pids(wpid, STUB).into_iter().any(|pid| !after_remove.contains(&pid)),
            Duration::from_secs(8),
        ),
        || format!("pids={:?}, absent={after_remove:?}", child_pids(wpid, STUB)),
    );
    checks.check(
        "hotplug add: renderers bounded by monitor count (no leak)",
        wait_until(|| count() <= 3, Duration::from_secs(8)),
        || format!("{} renderers for 3 monitors", count()),
    );

    client.call(
        "wall.apply",
        json!({ "type": "static", "path": img_str, "output": "*", "override_locks": true }),
        4,
    );
    checks.check(
        "walld recovers to a clean uniform apply after hotplug",
        wait_until(|| count() == 1 && names(&mut client).len() == 3, Duration::from_secs(8)),
        || format!("{} renderers, outs={:?}", count(), names(&mut client)),
    );
    checks.check("walld responsive after hotplug", walld.responsive(), String::new);
    checks.check("no panics in walld log", !walld.log_contents().contains("panicked"), String::new);

    for pid in child_pids(wpid, STUB) {
        let _ = std::process::Command::new("kill").arg("-9").arg(pid.to_string()).status();
    }
    if checks.failed() {
        sandbox.mark_failed();
    }
    checks.finish();
}
