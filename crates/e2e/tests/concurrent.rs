use serde_json::{Value, json};
use skwd_e2e::{
    Checks, Client, Sandbox, Walld, child_pids, ffmpeg_still, ffmpeg_video, field, wait_until,
    wall_outputs,
};
use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, Instant};
use wall_proto::rpc;

const STUB: &str = "fake_renderer";
const OUTS: usize = 3;
const BURST_ID_START: u64 = 1000;
const PIPELINE_APPLIES: usize = 64;
const PIPELINE_ID_START: u64 = 10_000;
const REPLY_TIMEOUT: Duration = Duration::from_secs(20);
const CATALOG_TIMEOUT: Duration = Duration::from_secs(15);

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
                let mut client = Client::connect(&socket).expect("burst connection");
                let id = BURST_ID_START + idx as u64;
                let path = params["path"].as_str().expect("burst path").to_string();
                let response = client.call(rpc::WALL_APPLY, params, id).expect("burst reply");
                assert_eq!(response["id"].as_u64(), Some(id));
                assert!(response.get("error").is_none(), "{response}");
                if response["result"]["superseded"] != true {
                    assert_eq!(response["result"]["applied"].as_str(), Some(path.as_str()));
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().expect("burst worker");
    }
}

fn reply(client: &mut Client, id: u64) -> Value {
    let deadline = Instant::now() + REPLY_TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "rpc timeout");
        let response = client.recv(remaining).expect("rpc reply");
        if response.get("event").is_some() {
            continue;
        }
        assert_eq!(response["id"].as_u64(), Some(id));
        assert!(response.get("error").is_none(), "{response}");
        return response;
    }
}

fn catalog(client: &mut Client, id: u64) -> HashSet<String> {
    client.send(rpc::WALL_LIST, json!({}), id);
    reply(client, id)["result"]["wallpapers"]
        .as_array()
        .expect("wallpaper rows")
        .iter()
        .map(|row| row["key"].as_str().expect("wallpaper key").to_string())
        .collect()
}

fn subscribe(client: &mut Client) {
    client.send(rpc::SUBSCRIBE, json!({}), 1);
    assert_eq!(reply(client, 1)["result"]["subscribed"], true);
}

fn pipeline(socket: &Path, pool: &[Wp]) -> String {
    let mut client = Client::connect(socket).expect("pipeline connection");
    for idx in 0..PIPELINE_APPLIES {
        let wp = &pool[idx % pool.len()];
        client.send(
            rpc::WALL_APPLY,
            json!({ "type": wp.kind, "path": wp.path, "output": "*" }),
            PIPELINE_ID_START + idx as u64,
        );
    }
    let terminal = PIPELINE_ID_START + PIPELINE_APPLIES as u64 - 1;
    let mut ids = HashSet::new();
    let deadline = Instant::now() + REPLY_TIMEOUT;
    for _ in 0..PIPELINE_APPLIES {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "pipeline timeout");
        let response = client.recv(remaining).expect("pipeline reply");
        let id = response["id"].as_u64().expect("pipeline id");
        assert!((PIPELINE_ID_START..=terminal).contains(&id));
        assert!(ids.insert(id), "duplicate id {id}");
        assert!(response.get("error").is_none(), "{response}");
        let path = &pool[(id - PIPELINE_ID_START) as usize % pool.len()].path;
        if response["result"]["superseded"] != true {
            assert_eq!(response["result"]["applied"].as_str(), Some(path.as_str()));
        }
        if id == terminal {
            assert_ne!(response["result"]["superseded"], true);
        }
    }
    assert_eq!(ids.len(), PIPELINE_APPLIES);
    pool[(PIPELINE_APPLIES - 1) % pool.len()].path.clone()
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn concurrent_applies_stay_coherent() {
    let stub_owned = skwd_e2e::stub_renderer!();
    let stub = stub_owned.as_str();
    let mut sandbox = Sandbox::new("concurrent");
    let lib = sandbox.library();
    let workshop = sandbox.root.join("steam-workshop");
    let assets = sandbox.root.join("steam-assets");
    std::fs::create_dir_all(&workshop).unwrap();
    std::fs::create_dir_all(&assets).unwrap();
    let mut pool = vec![
        Wp { kind: "static", path: lib.join("a.png").to_string_lossy().into_owned() },
        Wp { kind: "static", path: lib.join("b.png").to_string_lossy().into_owned() },
    ];
    assert!(ffmpeg_still(&lib.join("a.png"), "color=c=red:s=320x180"), "fixture a");
    assert!(ffmpeg_still(&lib.join("b.png"), "color=c=green:s=320x180"), "fixture b");
    for (name, color) in [("v.mp4", "blue"), ("w.mp4", "magenta")] {
        assert!(ffmpeg_video(&lib.join(name), color, 1.0), "video fixture");
        pool.push(Wp { kind: "video", path: lib.join(name).to_string_lossy().into_owned() });
    }

    sandbox.set_env("SKWD_FAKE_OUTPUTS", "DP-1:1920x1080,DP-2:2560x1440,DP-3:1920x1080");
    sandbox.set_env("SKWD_WALL_PAPER_STILL", stub);
    sandbox.set_env("SKWD_WALL_PAPER_VK", stub);
    sandbox.set_env("SKWD_WALL_LOG", "debug");
    let lib_str = lib.to_string_lossy().into_owned();
    sandbox.write_config(&json!({
        "paths": {
            "wallpaper": lib_str,
            "videoWallpaper": lib_str,
            "steamWorkshop": workshop,
            "steamWeAssets": assets,
            "paperBin": sandbox.root.join("missing-paper")
        },
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
    let expected: HashSet<String> = pool
        .iter()
        .map(|wp| {
            let name = Path::new(&wp.path).file_name().expect("fixture name").to_string_lossy();
            format!("{}:{name}", wp.kind)
        })
        .collect();
    let targets = ["*", "DP-1", "DP-2", "DP-3"];
    assert!(wait_until(|| expected.is_subset(&catalog(&mut client, 2)), CATALOG_TIMEOUT));

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

    let terminal = pipeline(&socket, &pool);
    checks.check(
        "pipelined applies reach the terminal wallpaper",
        wait_until(
            || uniform_wallpaper(&mut client).map(|uniform| uniform.1) == Some(terminal.clone()),
            CATALOG_TIMEOUT,
        ),
        || format!("{:?}", wall_outputs(&mut client)),
    );
    let mut fresh = walld.client();
    subscribe(&mut fresh);
    let fresh_keys = catalog(&mut fresh, 2);
    checks.check("reconnected catalog stays populated", expected.is_subset(&fresh_keys), || {
        format!("missing {:?}", expected.difference(&fresh_keys).collect::<Vec<_>>())
    });
    drop(fresh);

    let recovery = client
        .call(rpc::WALL_APPLY, json!({ "type": "static", "path": paths[0], "output": "*" }), 9000)
        .expect("recovery reply");
    assert_eq!(recovery["id"], 9000);
    assert!(recovery.get("error").is_none(), "{recovery}");
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
    drop(client);
    drop(walld);

    let restarted = Walld::start(&sandbox);
    let mut fresh = restarted.client();
    subscribe(&mut fresh);
    let mut restarted_keys = HashSet::new();
    checks.check(
        "restarted daemon retains the catalog",
        wait_until(
            || {
                restarted_keys = catalog(&mut fresh, 2);
                expected.is_subset(&restarted_keys)
            },
            CATALOG_TIMEOUT,
        ),
        || format!("missing {:?}", expected.difference(&restarted_keys).collect::<Vec<_>>()),
    );
    checks.check("restarted daemon stays responsive", restarted.responsive(), String::new);
    checks.check(
        "no panics after restart",
        !restarted.log_contents().contains("panicked"),
        String::new,
    );
    for pid in child_pids(restarted.pid(), STUB) {
        let _ = std::process::Command::new("kill").arg("-9").arg(pid.to_string()).status();
    }
    if checks.failed() {
        sandbox.mark_failed();
    }
    checks.finish();
}
