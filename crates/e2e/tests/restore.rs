use serde_json::json;
use skwd_e2e::{
    Checks, Client, Sandbox, Walld, child_pids, ffmpeg_still, ffmpeg_video, field, wait_until,
    wall_outputs,
};
use std::time::Duration;

const STUB: &str = "fake_renderer";

fn output_kind_path(client: &mut Client, name: &str) -> (String, String) {
    wall_outputs(client)
        .into_iter()
        .find(|out| field(out, "name") == name)
        .map_or_else(Default::default, |out| {
            (field(&out, "type").to_string(), field(&out, "path").to_string())
        })
}

fn reap_stubs(walld_pid: u32) {
    for pid in child_pids(walld_pid, STUB) {
        let _ = std::process::Command::new("kill").arg("-9").arg(pid.to_string()).status();
    }
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn restore_per_output() {
    let stub_owned = skwd_e2e::stub_renderer!();
    let stub = stub_owned.as_str();
    let mut sandbox = Sandbox::new("restore");
    let lib = sandbox.library();
    let img = lib.join("a.png");
    let vid = lib.join("v.mp4");
    assert!(ffmpeg_still(&img, "color=c=red:s=320x180"), "fixture");
    let have_video = ffmpeg_video(&vid, "blue", 1.0);
    let img_str = img.to_string_lossy().into_owned();
    let vid_str = vid.to_string_lossy().into_owned();

    sandbox.set_env("SKWD_FAKE_OUTPUTS", "DP-1:1920x1080,DP-2:2560x1440,DP-3:1920x1080");
    sandbox.set_env("SKWD_WALL_PAPER_STILL", stub);
    sandbox.set_env("SKWD_WALL_PAPER_VK", stub);
    sandbox.set_env("SKWD_WALL_LOG", "debug");
    let lib_str = lib.to_string_lossy().into_owned();
    sandbox.write_config(&json!({
        "paths": { "wallpaper": lib_str, "videoWallpaper": lib_str },
        "pickOnlyMode": false,
        "restoreOnStartup": true,
        "general": { "randomInterval": 0 },
        "effects": { "autoRecolor": false, "autoTheme": "" },
        "transition": { "enabled": false },
    }));

    let mut checks = Checks::default();

    let want_dp1 = if have_video {
        ("video".to_string(), vid_str.clone())
    } else {
        ("static".to_string(), img_str.clone())
    };
    let want_other = ("static".to_string(), img_str.clone());
    {
        let walld = Walld::start(&sandbox);
        let mut client = walld.client();
        client.call("wall.apply", json!({ "type": "static", "path": img_str, "output": "*" }), 1);
        if have_video {
            client.call(
                "wall.apply",
                json!({ "type": "video", "path": vid_str, "output": "DP-1", "mute": true }),
                2,
            );
        }
        let staged = wait_until(
            || output_kind_path(&mut client, "DP-1") == want_dp1,
            Duration::from_secs(8),
        );
        checks.check("pre-restart: DP-1 diverged wallpaper is live", staged, || {
            format!("{:?}", output_kind_path(&mut client, "DP-1"))
        });
        reap_stubs(walld.pid());
    }

    let walld = Walld::start(&sandbox);
    let mut client = walld.client();

    for name in ["DP-1", "DP-2", "DP-3"] {
        let want = if name == "DP-1" { &want_dp1 } else { &want_other };
        let restored =
            wait_until(|| output_kind_path(&mut client, name) == *want, Duration::from_secs(10));
        checks.check(&format!("{name} restores its persisted wallpaper"), restored, || {
            format!("got {:?}, wanted {want:?}", output_kind_path(&mut client, name))
        });
    }
    let respawned =
        wait_until(|| !child_pids(walld.pid(), STUB).is_empty(), Duration::from_secs(8));
    checks.check("restart respawns a renderer from persisted state", respawned, String::new);
    checks.check(
        "no panics across the restart",
        !walld.log_contents().contains("panicked"),
        String::new,
    );

    reap_stubs(walld.pid());
    if checks.failed() {
        sandbox.mark_failed();
    }
    checks.finish();
}
