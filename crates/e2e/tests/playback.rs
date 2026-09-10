use serde_json::{Value, json};
use skwd_e2e::{Sandbox, Walld, wait_until};
use std::os::unix::process::CommandExt;
use std::time::Duration;

fn playback(walld: &Walld) -> Value {
    walld.client().call("status", json!({}), 77).unwrap()["result"]["playback"].clone()
}

#[test]
#[ignore = "requires release daemon and process access"]
fn process_rules_reload_and_release_after_exit() {
    let sandbox = Sandbox::new("automatic-playback");
    let mut config = json!({"paths": {"wallpaper": sandbox.library(), "videoWallpaper": sandbox.library(), "steamWorkshop": sandbox.library()},
        "restoreOnStartup": false, "pickOnlyMode": true,
        "general": {"randomInterval": 0},
        "playback": {"processEnabled": false, "processes": "skwd-auto-pause-test", "resumeDelay": 1}});
    sandbox.write_config(&config);
    let walld = Walld::start(&sandbox);
    let mut process =
        std::process::Command::new("sleep").arg0("skwd-auto-pause-test").arg("30").spawn().unwrap();
    assert!(!playback(&walld)["automatic_paused"].as_bool().unwrap());
    config["playback"]["processEnabled"] = json!(true);
    sandbox.write_config(&config);
    let detected = wait_until(
        || playback(&walld)["processes"] == json!(["skwd-auto-pause-test"]),
        Duration::from_secs(8),
    );
    process.kill().unwrap();
    process.wait().unwrap();
    assert!(detected, "{}", walld.log_contents());
    assert!(
        wait_until(
            || {
                let state = playback(&walld);
                state["processes"] == json!([])
                    && state["resume_pending"] == true
                    && state["automatic_paused"] == true
            },
            Duration::from_secs(5)
        ),
        "resume delay was not held"
    );
    assert!(wait_until(|| playback(&walld)["automatic_paused"] == false, Duration::from_secs(5)));
    let processes = walld.client().call("playback.processes", json!({}), 78).unwrap();
    assert!(processes["result"]["processes"].is_array());
}

#[test]
#[ignore = "requires release daemon and renderer fixture"]
fn monitor_pause_splits_shared_video_and_scenes() {
    use skwd_e2e::{child_pids, ffmpeg_video, wall_outputs};
    for scene in [false, true] {
        let mut sandbox = Sandbox::new(if scene { "pause-scene" } else { "pause-video" });
        let stub = skwd_e2e::stub_renderer!();
        sandbox.set_env("SKWD_WALL_PAPER_VK", &stub);
        sandbox.set_env("SKWD_WALL_PAPER_STILL", &stub);
        sandbox.set_env("SKWD_FAKE_OUTPUTS", "DP-1:1920x1080,DP-2:1920x1080");
        let video = sandbox.library().join("test.mp4");
        assert!(ffmpeg_video(&video, "blue", 1.0));
        let we = sandbox.root.join("we").join("12345");
        std::fs::create_dir_all(&we).unwrap();
        std::fs::write(we.join("project.json"), r#"{"type":"scene","title":"test"}"#).unwrap();
        std::fs::write(we.join("scene.pkg"), b"fixture").unwrap();
        sandbox.write_config(&json!({
            "paths": {"wallpaper": sandbox.library(), "videoWallpaper": sandbox.library(), "steamWorkshop": sandbox.root.join("we")},
            "restoreOnStartup": false, "general": {"randomInterval": 0}, "theme": {"policy": "off"}, "transition": {"enabled": false}
        }));
        let walld = Walld::start(&sandbox);
        let mut client = walld.client();
        let params = if scene {
            json!({"type":"we","we_id":"12345"})
        } else {
            json!({"type":"video","path":video})
        };
        let response = client.call("wall.apply", params.clone(), 20).unwrap();
        assert!(response.get("error").is_none(), "{response}");
        assert!(wait_until(
            || child_pids(walld.pid(), "fake_renderer").len() == 1,
            Duration::from_secs(8)
        ));
        let response =
            client.call("wall.set_paused", json!({"output":"DP-1","paused":true}), 21).unwrap();
        assert!(response.get("error").is_none(), "{response}\n{}", walld.log_contents());
        assert!(
            wait_until(
                || child_pids(walld.pid(), "fake_renderer").len() == 2,
                Duration::from_secs(8)
            ),
            "{}",
            walld.log_contents()
        );
        let mut outputs = wall_outputs(&mut client);
        outputs.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        assert_eq!(outputs[0]["paused"], true);
        assert_eq!(outputs[1]["paused"], false);
        client.call("wall.set_paused", json!({"output":"DP-2","paused":true}), 22);
        client.call("wall.set_paused", json!({"output":"DP-1","paused":false}), 23);
        outputs = wall_outputs(&mut client);
        assert!(outputs.iter().any(|o| o["name"] == "DP-1" && o["paused"] == false));
        assert!(outputs.iter().any(|o| o["name"] == "DP-2" && o["paused"] == true));
        client.call("wall.apply", params, 24);
        assert!(wait_until(
            || child_pids(walld.pid(), "fake_renderer").len() == 2,
            Duration::from_secs(8)
        ));
        outputs = wall_outputs(&mut client);
        assert!(outputs.iter().any(|o| o["name"] == "DP-2" && o["manual_paused"] == true));
        client.call("wall.set_paused", json!({"paused":true}), 25);
        client.call("wall.set_paused", json!({"output":"DP-1","paused":false}), 26);
        outputs = wall_outputs(&mut client);
        assert!(outputs.iter().any(|o| o["name"] == "DP-1" && o["paused"] == false));
        assert!(outputs.iter().any(|o| o["name"] == "DP-2" && o["paused"] == true));
        let bad = client
            .call("wall.set_paused", json!({"output":"not-a-monitor","paused":true}), 27)
            .unwrap();
        assert!(bad.get("error").is_some());
        let still = sandbox.library().join("still.png");
        assert!(skwd_e2e::ffmpeg_still(&still, "color=c=red:s=320x180"));
        let response =
            client.call("wall.apply", json!({"type":"static","path":still}), 28).unwrap();
        assert!(response.get("error").is_none(), "{response}");
        client.call("wall.set_paused", json!({"paused":true}), 29);
        outputs = wall_outputs(&mut client);
        assert!(
            outputs.iter().all(|o| o["type"] == "static"
                && o["paused"] == false
                && o["manual_paused"] == false)
        );
    }
}

#[test]
#[ignore = "requires release daemon and renderer fixture"]
fn configured_layer_refreshes_live_video_and_scene() {
    use skwd_e2e::{child_pids, ffmpeg_video};
    for scene in [false, true] {
        let mut sandbox = Sandbox::new("wallpaper-layer");
        let stub = skwd_e2e::stub_renderer!();
        sandbox.set_env("SKWD_WALL_PAPER_VK", &stub);
        sandbox.set_env("SKWD_FAKE_OUTPUTS", "DP-1:1920x1080");
        let video = sandbox.library().join("test.mp4");
        assert!(ffmpeg_video(&video, "blue", 1.0));
        let we = sandbox.root.join("we/12345");
        std::fs::create_dir_all(&we).unwrap();
        std::fs::write(we.join("project.json"), r#"{"type":"scene","title":"test"}"#).unwrap();
        std::fs::write(we.join("scene.pkg"), b"fixture").unwrap();
        let mut config = json!({"paths":{"wallpaper":sandbox.library(),"videoWallpaper":sandbox.library(),"steamWorkshop":sandbox.root.join("we")}, "restoreOnStartup":false,"general":{"randomInterval":0},"theme":{"policy":"off"},"transition":{"enabled":false}, "paper":{"wallpaperLayer":"bottom"}});
        sandbox.write_config(&config);
        let walld = Walld::start(&sandbox);
        let params = if scene {
            json!({"type":"we","we_id":"12345"})
        } else {
            json!({"type":"video","path":video})
        };
        let response = walld.client().call("wall.apply", params, 30).unwrap();
        assert!(response.get("error").is_none(), "{response}");
        for layer in ["bottom", "background", "top", "overlay"] {
            config["paper"]["wallpaperLayer"] = json!(layer);
            sandbox.write_config(&config);
            assert!(
                wait_until(
                    || {
                        let pids = child_pids(walld.pid(), "fake_renderer");
                        !pids.is_empty()
                            && pids.iter().all(|pid| {
                                std::fs::read(format!("/proc/{pid}/environ")).is_ok_and(|env| {
                                    env.split(|byte| *byte == 0).any(|entry| {
                                        entry == format!("SKWD_VK_LAYER={layer}").as_bytes()
                                    })
                                })
                            })
                    },
                    Duration::from_secs(12)
                ),
                "{layer}: {}",
                walld.log_contents()
            );
        }
    }
}

#[test]
#[ignore = "requires release daemon and Niri event fixture"]
fn overview_pause_releases_without_overriding_manual_pause() {
    use std::io::{BufRead, Write};
    let mut sandbox = Sandbox::new("overview-playback");
    let stub = skwd_e2e::stub_renderer!();
    sandbox.set_env("SKWD_WALL_PAPER_VK", &stub);
    let video = sandbox.library().join("overview.mp4");
    assert!(skwd_e2e::ffmpeg_video(&video, "blue", 1.0));
    let socket = sandbox.root.join("niri.sock");
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    sandbox.set_env("NIRI_SOCKET", socket.to_str().unwrap());
    sandbox.set_env("SKWD_FAKE_OUTPUTS", "DP-1:1920x1080");
    let (send, receive) = std::sync::mpsc::channel::<bool>();
    let server = std::thread::spawn(move || {
        let mut stream = loop {
            let (stream, _) = listener.accept().unwrap();
            let mut command = String::new();
            std::io::BufReader::new(stream.try_clone().unwrap()).read_line(&mut command).unwrap();
            if command.is_empty() {
                continue;
            }
            assert_eq!(command.trim(), "\"EventStream\"");
            break stream;
        };
        for open in receive {
            writeln!(stream, "{}", json!({"OverviewOpenedOrClosed":{"is_open":open}})).unwrap();
        }
    });
    sandbox.write_config(&json!({"paths":{"wallpaper":sandbox.library()},"restoreOnStartup":false,"general":{"randomInterval":0},"paper":{"wallpaperLayer":"background"},"niri":{"overviewOnlyPlayback":true}}));
    let walld = Walld::start(&sandbox);
    let applied =
        walld.client().call("wall.apply", json!({"type":"video","path":video}), 39).unwrap();
    assert!(applied.get("error").is_none(), "{applied}");
    send.send(false).unwrap();
    assert!(wait_until(|| playback(&walld)["overview_paused"] == true, Duration::from_secs(8)));
    walld.client().call("wall.set_paused", json!({"paused":true}), 40);
    send.send(true).unwrap();
    assert!(wait_until(|| playback(&walld)["overview_paused"] == false, Duration::from_secs(5)));
    let outputs = skwd_e2e::wall_outputs(&mut walld.client());
    assert!(
        outputs.iter().any(|output| output["paused"] == true && output["manual_paused"] == true)
    );
    let manual = walld.client().call("wall.set_paused", json!({"paused":false}), 41).unwrap();
    assert!(manual.get("error").is_none());
    send.send(false).unwrap();
    assert!(wait_until(|| playback(&walld)["overview_paused"] == true, Duration::from_secs(5)));
    drop(send);
    server.join().unwrap();
    assert!(wait_until(|| playback(&walld)["overview_paused"] == false, Duration::from_secs(5)));
}

#[test]
#[ignore = "requires release daemon and Niri event fixture"]
fn niri_columns_follow_visible_workspace_and_preserve_manual_pause() {
    use std::io::{BufRead, Write};
    let mut sandbox = Sandbox::new("column-playback");
    sandbox.set_env("SKWD_WALL_PAPER_VK", &skwd_e2e::stub_renderer!());
    sandbox.set_env("SKWD_FAKE_OUTPUTS", "DP-1:1920x1080,DP-2:1920x1080");
    let video = sandbox.library().join("columns.mp4");
    assert!(skwd_e2e::ffmpeg_video(&video, "blue", 1.0));
    let socket = sandbox.root.join("niri.sock");
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    sandbox.set_env("NIRI_SOCKET", socket.to_str().unwrap());
    let (send, receive) = std::sync::mpsc::channel::<Value>();
    let server = std::thread::spawn(move || {
        let mut events = None;
        loop {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut command = String::new();
                std::io::BufReader::new(stream.try_clone().unwrap())
                    .read_line(&mut command)
                    .unwrap();
                match command.trim() {
                    "" => {}
                    "\"EventStream\"" => events = Some(stream),
                    "\"Outputs\"" => {
                        writeln!(stream, "{}", json!({"Ok":{"Outputs":{"DP-1":{"logical":{"width":1920,"height":1080}},"DP-2":{"logical":{"width":1920,"height":1080}}}}})).unwrap();
                    }
                    other => panic!("unexpected Niri request {other}"),
                }
            }
            if let Some(stream) = events.as_mut() {
                for value in receive.try_iter() {
                    if value.is_null() {
                        return;
                    }
                    writeln!(stream, "{value}").unwrap();
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    sandbox.write_config(&json!({"paths":{"wallpaper":sandbox.library()},"restoreOnStartup":false,"niri":{"fullWidthPause":true},"playback":{"fullscreenScope":"display","resumeDelay":0.2}}));
    let walld = Walld::start(&sandbox);
    walld.client().call("wall.apply", json!({"type":"video","path":video}), 1).unwrap();
    send.send(json!({"WorkspacesChanged":{"workspaces":[{"id":1,"output":"DP-1","is_active":true},{"id":2,"output":"DP-1","is_active":false}]}})).unwrap();
    send.send(json!({"WindowsChanged":{"windows":[{"id":8,"workspace_id":1,"is_floating":false,"layout":{"pos_in_scrolling_layout":[1,1],"tile_size":[1888,1048],"tile_pos_in_workspace_view":[16,16]}}]}})).unwrap();
    assert!(wait_until(|| playback(&walld)["full_width_paused"] == true, Duration::from_secs(8)));
    assert!(wait_until(
        || {
            let outputs = skwd_e2e::wall_outputs(&mut walld.client());
            outputs.len() == 2 && outputs.iter().all(|o| o["paused"] == (o["name"] == "DP-1"))
        },
        Duration::from_secs(5)
    ));
    send.send(json!({"WorkspaceActivated":{"id":2,"focused":true}})).unwrap();
    assert!(wait_until(|| playback(&walld)["resume_pending"] == true, Duration::from_secs(5)));
    assert!(wait_until(|| playback(&walld)["automatic_paused"] == false, Duration::from_secs(5)));
    walld.client().call("wall.set_paused", json!({"paused":true}), 2).unwrap();
    send.send(json!({"WorkspaceActivated":{"id":1,"focused":true}})).unwrap();
    assert!(wait_until(|| playback(&walld)["full_width_paused"] == true, Duration::from_secs(5)));
    send.send(Value::Null).unwrap();
    server.join().unwrap();
    assert!(wait_until(|| playback(&walld)["automatic_paused"] == false, Duration::from_secs(5)));
    assert!(
        skwd_e2e::wall_outputs(&mut walld.client())
            .iter()
            .all(|o| o["paused"] == true && o["manual_paused"] == true)
    );
}
