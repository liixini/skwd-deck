use serde_json::{Value, json};
use skwd_e2e::{
    Checks, Client, Sandbox, Walld, child_pids, ffmpeg_still, ffmpeg_video, field, wait_until,
    wall_outputs,
};
use std::path::Path;
use std::time::Duration;

const STUB: &str = "fake_renderer";
const OUTS: usize = 3;

struct Wp {
    kind: &'static str,
    path: String,
}

fn uniform_wallpaper(client: &mut Client) -> Option<(String, String)> {
    let outs = wall_outputs(client);
    if outs.len() != OUTS {
        return None;
    }
    let first = (field(&outs[0], "type").to_string(), field(&outs[0], "path").to_string());
    outs.iter()
        .all(|out| (field(out, "type"), field(out, "path")) == (&first.0, &first.1))
        .then_some(first)
}

fn burst(socket: &Path, applies: &[Value]) {
    let workers: Vec<_> = applies
        .iter()
        .cloned()
        .enumerate()
        .map(|(idx, params)| {
            let socket = socket.to_path_buf();
            std::thread::spawn(move || {
                if let Some(mut client) = Client::connect(&socket) {
                    client.call("wall.apply", params, 1000 + idx as u64);
                }
            })
        })
        .collect();
    for worker in workers {
        let _ = worker.join();
    }
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn concurrent_applies_stay_coherent() {
    let stub_owned = skwd_e2e::stub_renderer!();
    let stub = stub_owned.as_str();
    let mut sandbox = Sandbox::new("concurrent");
    let lib = sandbox.library();
    let mut pool = vec![
        Wp { kind: "static", path: lib.join("a.png").to_string_lossy().into_owned() },
        Wp { kind: "static", path: lib.join("b.png").to_string_lossy().into_owned() },
    ];
    assert!(ffmpeg_still(&lib.join("a.png"), "color=c=red:s=320x180"), "fixture a");
    assert!(ffmpeg_still(&lib.join("b.png"), "color=c=green:s=320x180"), "fixture b");
    for (name, color) in [("v.mp4", "blue"), ("w.mp4", "magenta")] {
        if ffmpeg_video(&lib.join(name), color, 1.0) {
            pool.push(Wp { kind: "video", path: lib.join(name).to_string_lossy().into_owned() });
        }
    }

    sandbox.set_env("SKWD_FAKE_OUTPUTS", "DP-1:1920x1080,DP-2:2560x1440,DP-3:1920x1080");
    sandbox.set_env("SKWD_WALL_PAPER_STILL", stub);
    sandbox.set_env("SKWD_WALL_PAPER_VK", stub);
    sandbox.set_env("SKWD_WALL_LOG", "debug");
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
    let socket = sandbox.socket();
    let mut client = walld.client();
    let mut checks = Checks::default();
    let paths: Vec<String> = pool.iter().map(|wp| wp.path.clone()).collect();
    let targets = ["*", "DP-1", "DP-2", "DP-3"];

    for round in 0..8 {
        let uniform_applies: Vec<Value> = pool
            .iter()
            .map(|wp| json!({ "type": wp.kind, "path": wp.path, "output": "*" }))
            .collect();
        burst(&socket, &uniform_applies);
        let converged = wait_until(
            || uniform_wallpaper(&mut client).is_some() && child_pids(wpid, STUB).len() == 1,
            Duration::from_secs(12),
        );
        assert!(
            converged,
            "round {round}: {} applies, no uniform renderer\n  outs={:?}\n  renderers={}",
            uniform_applies.len(),
            wall_outputs(&mut client),
            child_pids(wpid, STUB).len(),
        );
        let winner = uniform_wallpaper(&mut client).unwrap();
        assert!(paths.contains(&winner.1), "round {round}: unapplied winner {winner:?}");
    }
    checks.check("concurrent uniform applies always converge coherently", true, String::new);

    for round in 0..8 {
        let mixed: Vec<Value> = (0..6)
            .map(|idx| {
                let wp = &pool[idx % pool.len()];
                let target = targets[idx % targets.len()];
                json!({ "type": wp.kind, "path": wp.path, "output": target })
            })
            .collect();
        burst(&socket, &mixed);
        let bounded = wait_until(|| child_pids(wpid, STUB).len() <= OUTS, Duration::from_secs(12));
        assert!(
            bounded,
            "round {round}: leaked renderers {} > {OUTS}",
            child_pids(wpid, STUB).len()
        );
        assert!(
            sandbox.outputs_json().is_object(),
            "round {round}: outputs.json not an object\n  {}",
            sandbox.outputs_json()
        );
        for out in wall_outputs(&mut client) {
            let path = field(&out, "path");
            assert!(
                path.is_empty() || paths.iter().any(|known| known == path),
                "round {round}: {} shows unknown {path}",
                field(&out, "name"),
            );
        }
    }
    checks.check("mixed concurrent bursts never leak or tear state", true, String::new);

    client.call("wall.apply", json!({ "type": "static", "path": paths[0], "output": "*" }), 9000);
    checks.check(
        "walld recovers to a clean apply after the concurrent storm",
        wait_until(
            || uniform_wallpaper(&mut client).map(|uniform| uniform.1) == Some(paths[0].clone()),
            Duration::from_secs(10),
        ),
        || format!("{:?}", wall_outputs(&mut client)),
    );
    checks.check("walld responsive after the storm", walld.responsive(), String::new);
    checks.check("no panics in walld log", !walld.log_contents().contains("panicked"), String::new);

    for pid in child_pids(wpid, STUB) {
        let _ = std::process::Command::new("kill").arg("-9").arg(pid.to_string()).status();
    }
    if checks.failed() {
        sandbox.mark_failed();
    }
    checks.finish();
}
