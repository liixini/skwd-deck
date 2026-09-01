use serde_json::json;
use skwd_e2e::{Checks, Client, Sandbox, Walld, ffmpeg_still, procs_with_env, wait_until};
use std::process::Stdio;
use std::time::Duration;

const RENDERERS: [&str; 2] = ["skwd-wall-still", "skwd-wall-vk"];

fn pick_only_config(sandbox: &Sandbox, wallpaper: &str) {
    sandbox.write_config(&json!({
        "paths": { "wallpaper": wallpaper, "videoWallpaper": wallpaper },
        "pickOnlyMode": true,
        "restoreOnStartup": false,
        "general": { "randomInterval": 0 },
        "effects": { "autoRecolor": false, "autoTheme": "" },
    }));
}

fn socket_guard(checks: &mut Checks) {
    let sandbox = Sandbox::new("lifecycle-guard");
    pick_only_config(&sandbox, &sandbox.library().to_string_lossy());
    let walld = Walld::start(&sandbox);
    checks.check("walld up before guard test", walld.responsive(), String::new);

    let mut cmd = sandbox.walld_command();
    cmd.stdout(Stdio::null()).stderr(Stdio::null()).stdin(Stdio::null());
    let mut second = cmd.spawn().expect("spawn second walld");
    let exited = wait_until(|| matches!(second.try_wait(), Ok(Some(_))), Duration::from_secs(6));
    checks.check("second walld exits (socket already owned)", exited, || {
        "second instance still alive after 6s".into()
    });
    if !exited {
        let _ = second.kill();
    }
    let _ = second.wait();
    checks.check("original walld survives the intrusion", walld.responsive(), String::new);
}

fn missing_media_dir(checks: &mut Checks) {
    let sandbox = Sandbox::new("lifecycle-dir");
    let missing = sandbox.root.join("does-not-exist-yet/wallpapers");
    pick_only_config(&sandbox, &missing.to_string_lossy());
    checks.check("wallpaper dir absent before startup", !missing.exists(), String::new);
    let walld = Walld::start(&sandbox);
    let created = wait_until(|| missing.is_dir(), Duration::from_secs(4));
    checks.check("walld creates the missing wallpaper dir on startup", created, || {
        format!("{} still absent", missing.display())
    });
    checks.check("walld responsive after dir creation", walld.responsive(), String::new);
}

fn pick_only_and_resilience(checks: &mut Checks) {
    let sandbox = Sandbox::new("lifecycle-apply");
    let lib = sandbox.library();
    let img = lib.join("wall.png");
    assert!(ffmpeg_still(&img, "color=c=red:s=320x180"), "ffmpeg fixture");
    pick_only_config(&sandbox, &lib.to_string_lossy());
    let walld = Walld::start(&sandbox);
    let sock = sandbox.socket().to_string_lossy().into_owned();
    let img_str = img.to_string_lossy().into_owned();
    let mut client = walld.client();

    let resp = client.call("wall.apply", json!({ "type": "static", "path": img_str }), 1);
    checks.check(
        "pickOnly apply returns a result",
        resp.as_ref().is_some_and(|value| value.get("result").is_some()),
        || format!("{resp:?}"),
    );
    std::thread::sleep(Duration::from_millis(800));
    let leaked = procs_with_env(&RENDERERS, &sock);
    checks.check("pickOnly apply spawns no renderer", leaked.is_empty(), || format!("{leaked:?}"));

    client.call("wall.apply", json!({ "type": "static", "path": img_str, "output": "DP-NOPE" }), 2);
    checks.check(
        "apply to a nonexistent output keeps walld responsive",
        walld.responsive(),
        String::new,
    );
    client.call("wall.apply", json!({ "type": "static", "path": "/nonexistent/file/xyz.webp" }), 3);
    checks.check("apply of a missing file keeps walld responsive", walld.responsive(), String::new);

    let socket = sandbox.socket();
    let workers: Vec<_> = (0..5)
        .map(|worker| {
            let socket = socket.clone();
            let path = img_str.clone();
            std::thread::spawn(move || {
                if let Some(mut conn) = Client::connect(&socket) {
                    conn.call(
                        "wall.apply",
                        json!({ "type": "static", "path": path }),
                        100 + worker,
                    );
                }
            })
        })
        .collect();
    for worker in workers {
        let _ = worker.join();
    }
    checks.check("walld responsive after 5 concurrent applies", walld.responsive(), String::new);
    checks.check(
        "no renderer leaked across the concurrent burst",
        procs_with_env(&RENDERERS, &sock).is_empty(),
        String::new,
    );
    checks.check("no panics in walld log", !walld.log_contents().contains("panicked"), String::new);
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn walld_lifecycle_headless() {
    let mut checks = Checks::default();
    socket_guard(&mut checks);
    missing_media_dir(&mut checks);
    pick_only_and_resilience(&mut checks);
    checks.finish();
}
