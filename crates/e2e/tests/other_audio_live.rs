use serde_json::{Value, json};
use skwd_e2e::{Sandbox, Walld, wait_until};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct AudioClient(Child);

impl Drop for AudioClient {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn mpv(server: &str, sink: &str, clip: &str) -> AudioClient {
    AudioClient(
        Command::new("mpv")
            .args([
                "--no-config",
                "--video=no",
                "--ao=pulse",
                "--loop-file=inf",
                "--force-media-title=skwd-demo.mp4",
                clip,
            ])
            .env("PULSE_SERVER", server)
            .env("PULSE_SINK", sink)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start real mpv audio"),
    )
}

fn sink_inputs(server: &str) -> Value {
    let output = Command::new("pactl")
        .args(["-f", "json", "list", "sink-inputs"])
        .env("PULSE_SERVER", server)
        .output()
        .unwrap();
    assert!(output.status.success());
    serde_json::from_slice(&output.stdout).unwrap()
}

fn stream(server: &str, pid: u32) -> Option<Value> {
    let pid = pid.to_string();
    sink_inputs(server)
        .as_array()
        .unwrap()
        .iter()
        .find(|input| input["properties"]["application.process.id"].as_str() == Some(pid.as_str()))
        .cloned()
}

fn measured_rms(server: &str, sink: &str, sandbox: &Sandbox) -> f32 {
    let path = sandbox.root.join("other-audio.f32le");
    let output = std::fs::File::create(&path).unwrap();
    let capture = AudioClient(
        Command::new("parec")
            .args([
                "--device",
                &format!("{sink}.monitor"),
                "--format=float32le",
                "--rate=48000",
                "--channels=2",
                "--latency-msec=50",
            ])
            .env("PULSE_SERVER", server)
            .stdin(Stdio::null())
            .stdout(output)
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    assert!(wait_until(
        || std::fs::metadata(&path).is_ok_and(|metadata| metadata.len() >= 96_000),
        Duration::from_secs(5)
    ));
    drop(capture);
    let bytes = std::fs::read(path).unwrap();
    let samples: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|sample| f32::from_le_bytes(sample.try_into().unwrap()))
        .collect();
    assert!(samples.len() >= 24_000, "monitor capture too short");
    (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32).sqrt()
}

fn observed(walld: &Walld, expected: bool) -> bool {
    let status = walld.client().call("status", json!({}), 91).unwrap();
    let playback = &status["result"]["playback"];
    playback["other_audio_supported"] == true
        && playback["other_audio_playing"] == expected
        && playback["audio_ducked"] == expected
}

#[test]
#[ignore = "requires SKWD_TEST_PULSE_SERVER, SKWD_TEST_PULSE_SINK, SKWD_TEST_AUDIO_CLIP and release daemon"]
fn real_audio_is_detected_at_startup_and_after_stream_events() {
    let server = std::env::var("SKWD_TEST_PULSE_SERVER").expect("explicit private Pulse server");
    let sink = std::env::var("SKWD_TEST_PULSE_SINK").expect("private null sink");
    let clip = std::env::var("SKWD_TEST_AUDIO_CLIP").expect("steady audible looping audio clip");
    assert!(server.starts_with("unix:"), "use a private Unix Pulse server");
    let mut sandbox = Sandbox::new("other-audio-live");
    sandbox.set_env("PULSE_SERVER", &server);
    sandbox.write_config(&json!({
        "paths": {"wallpaper": sandbox.library(), "videoWallpaper": sandbox.library(), "steamWorkshop": sandbox.library()},
        "restoreOnStartup": false, "pickOnlyMode": true,
        "general": {"randomInterval": 0}, "theme": {"policy": "off"},
        "playback": {"muteOnOtherAudio": true}
    }));
    let player = mpv(&server, &sink, &clip);
    let player_pid = player.0.id();
    assert!(wait_until(
        || stream(&server, player_pid).is_some_and(|input| {
            input["properties"]["media.name"] == "skwd-demo.mp4 - mpv" && input["corked"] == false
        }),
        Duration::from_secs(5)
    ));
    let rms = measured_rms(&server, &sink, &sandbox);
    assert!(rms > 0.01, "real stream emitted no usable sound: RMS {rms}");
    eprintln!("real mpv stream RMS {rms}");
    let walld = Walld::start(&sandbox);
    assert!(
        wait_until(|| observed(&walld, true), Duration::from_secs(5)),
        "{}",
        walld.log_contents()
    );
    drop(player);
    assert!(wait_until(|| stream(&server, player_pid).is_none(), Duration::from_secs(5)));
    std::thread::sleep(Duration::from_millis(300));
    assert!(observed(&walld, true), "release grace was skipped");
    assert!(
        wait_until(|| observed(&walld, false), Duration::from_secs(3)),
        "remaining streams: {}\n{}",
        sink_inputs(&server),
        walld.log_contents()
    );
    let player = mpv(&server, &sink, &clip);
    let player_pid = player.0.id();
    assert!(wait_until(
        || stream(&server, player_pid).is_some_and(|input| input["corked"] == false),
        Duration::from_secs(5)
    ));
    assert!(
        wait_until(|| observed(&walld, true), Duration::from_secs(5)),
        "new stream not detected"
    );
    drop(player);
    assert!(wait_until(|| stream(&server, player_pid).is_none(), Duration::from_secs(5)));
    let removed_at = Instant::now();
    std::thread::sleep(Duration::from_millis(300));
    let player = mpv(&server, &sink, &clip);
    let player_pid = player.0.id();
    assert!(wait_until(
        || stream(&server, player_pid).is_some_and(|input| input["corked"] == false),
        Duration::from_secs(5)
    ));
    assert!(removed_at.elapsed() < Duration::from_secs(1), "new stream missed the grace window");
    let until = Instant::now() + Duration::from_secs(2);
    while Instant::now() < until {
        assert!(observed(&walld, true), "new stream failed to cancel release grace");
        std::thread::sleep(Duration::from_millis(50));
    }
    drop(player);
    assert!(wait_until(|| stream(&server, player_pid).is_none(), Duration::from_secs(5)));
    assert!(
        wait_until(|| observed(&walld, false), Duration::from_secs(3)),
        "remaining streams: {}\n{}",
        sink_inputs(&server),
        walld.log_contents()
    );
}
