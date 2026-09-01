use serde_json::{Value, json};
use skwd_e2e::{
    Checks, Sandbox, Walld, child_pids, ffmpeg_still, ffmpeg_video, field, wait_until, wall_outputs,
};
use std::collections::BTreeMap;
use std::time::Duration;

const STUB: &str = "fake_renderer";
const OUTS: [&str; 3] = ["DP-1", "DP-2", "DP-3"];

struct Wp {
    kind: &'static str,
    path: String,
}

struct Model {
    outs: BTreeMap<String, (String, String)>,
}

impl Model {
    fn new() -> Self {
        let outs =
            OUTS.iter().map(|name| ((*name).to_string(), (String::new(), String::new()))).collect();
        Self { outs }
    }

    fn apply(&mut self, target: &str, wp: &Wp) {
        let value = (wp.kind.to_string(), wp.path.clone());
        if target == "*" {
            for slot in self.outs.values_mut() {
                *slot = value.clone();
            }
        } else {
            self.outs.insert(target.to_string(), value);
        }
    }
}

struct Rng(u64);
impl Rng {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[(self.next_u64() as usize) % items.len()]
    }
}

fn model_matches(outputs: &[Value], model: &Model) -> bool {
    model.outs.iter().all(|(name, (kind, path))| {
        outputs
            .iter()
            .find(|out| field(out, "name") == name)
            .is_some_and(|out| field(out, "type") == kind && field(out, "path") == path)
    })
}

fn dedup_holds(outputs_json: &Value) -> bool {
    let Some(entries) = outputs_json.as_object() else {
        return true;
    };
    let mut unmuted_per_video: BTreeMap<&str, u32> = BTreeMap::new();
    for entry in entries.values() {
        let unmuted = entry.get("mute").and_then(Value::as_bool) == Some(false);
        if field(entry, "type") == "video" && unmuted {
            *unmuted_per_video.entry(field(entry, "path")).or_default() += 1;
        }
    }
    unmuted_per_video.values().all(|count| *count <= 1)
}

fn run_model(name: &str, transitions: bool, seed: u64) {
    let stub_owned = skwd_e2e::stub_renderer!();
    let stub = stub_owned.as_str();
    let mut sandbox = Sandbox::new(name);
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
        "transition": { "enabled": transitions, "durationMs": 120 },
    }));

    let walld = Walld::start(&sandbox);
    let wpid = walld.pid();
    let mut client = walld.client();
    let mut model = Model::new();
    let mut rng = Rng(seed);
    let targets = ["*", "DP-1", "DP-2", "DP-3"];

    let steps = 60;
    for step in 0..steps {
        let wp = rng.pick(&pool);
        let target = *rng.pick(&targets);
        let response = client.call(
            "wall.apply",
            json!({ "type": wp.kind, "path": wp.path, "output": target, "mute": false, "volume": 60 }),
            step + 1,
        );
        let applied = response.as_ref().is_some_and(|response| {
            response.get("result").is_some() && response.get("error").is_none()
        });
        if !applied {
            sandbox.mark_failed();
        }
        assert!(
            applied,
            "step {step} ({target} <- {} {}): apply failed {response:?}",
            wp.kind, wp.path,
        );
        model.apply(target, wp);

        let settled = wait_until(
            || model_matches(&wall_outputs(&mut client), &model),
            Duration::from_secs(10),
        );
        let outputs = wall_outputs(&mut client);
        if !settled {
            sandbox.mark_failed();
        }
        assert!(
            settled,
            "step {step} ({target} <- {} {}): model mismatch\n  model = {:?}\n  live  = {outputs:?}",
            wp.kind, wp.path, model.outs,
        );

        assert!(
            dedup_holds(&sandbox.outputs_json()),
            "step {step}: two unmuted audio records\n  {}",
            sandbox.outputs_json()
        );

        if target == "*" {
            let collapsed =
                wait_until(|| child_pids(wpid, STUB).len() == 1, Duration::from_secs(8));
            assert!(
                collapsed,
                "step {step}: '*' left {} renderers\n  {outputs:?}",
                child_pids(wpid, STUB).len(),
            );
        } else {
            let bounded =
                wait_until(|| child_pids(wpid, STUB).len() <= OUTS.len(), Duration::from_secs(8));
            if !bounded {
                sandbox.mark_failed();
                let log = walld.log_contents();
                let tail: Vec<&str> = log.lines().rev().take(30).collect();
                eprintln!("=== walld log tail ===");
                for line in tail.iter().rev() {
                    eprintln!("{line}");
                }
            }
            assert!(
                bounded,
                "step {step} ({target} <- {} {}): {} renderers > {} outputs\n  {outputs:?}",
                wp.kind,
                wp.path.rsplit('/').next().unwrap_or(""),
                child_pids(wpid, STUB).len(),
                OUTS.len(),
            );
        }
    }

    let mut checks = Checks::default();
    checks.check(&format!("{steps} random apply ops held every invariant"), true, String::new);
    checks.check("walld responsive after the sequence", walld.responsive(), String::new);
    checks.check("no panics in walld log", !walld.log_contents().contains("panicked"), String::new);

    for pid in child_pids(wpid, STUB) {
        let _ = std::process::Command::new("kill").arg("-9").arg(pid.to_string()).status();
    }
    if checks.failed() {
        sandbox.mark_failed();
    }
    checks.finish();
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn apply_per_output_model() {
    run_model("apply-model", false, 0x9E37_79B9_7F4A_7C15);
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn apply_model_with_transitions() {
    run_model("apply-model-trans", true, 0x2545_F491_4F6C_DD1D);
}
