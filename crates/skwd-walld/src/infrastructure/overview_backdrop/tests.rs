#![cfg(test)]

use super::{parse_backdrop_source, pick_backdrop_source};

#[test]
fn follow_vs_fixed() {
    let last = r#"{"type":"static","path":"/w/desk.png","thumb":"/w/desk.png"}"#;
    assert_eq!(pick_backdrop_source(true, "/w/fixed.png", last).as_deref(), Some("/w/desk.png"));
    assert_eq!(pick_backdrop_source(false, "/w/fixed.png", last).as_deref(), Some("/w/fixed.png"));
    assert_eq!(pick_backdrop_source(false, "  ", last).as_deref(), Some("/w/desk.png"));
}

#[test]
fn preserves_media_source() {
    assert_eq!(
        parse_backdrop_source(r#"{"type":"video","path":"/w/clip.mp4","thumb":"/c/t.jpg"}"#)
            .as_deref(),
        Some("/w/clip.mp4"),
    );
    assert_eq!(
        parse_backdrop_source(r#"{"type":"static","path":"/w/a.png","thumb":""}"#).as_deref(),
        Some("/w/a.png"),
    );
    assert_eq!(parse_backdrop_source(r#"{"type":"we","path":"","thumb":""}"#), None);
    assert_eq!(parse_backdrop_source("not json"), None);
}

#[test]
fn follows_wallpaper_engine_source() {
    assert_eq!(parse_backdrop_source(r#"{"type":"we","we_id":"3260370312","path":"/we/video.mp4","thumb":"/cache/preview.png"}"#).as_deref(), Some("we:3260370312"));
}

#[test]
fn resolves_media_without_using_previews() {
    let root = tempfile::tempdir().unwrap();
    let config = skwd_wall_core::config::Config::from_root(serde_json::json!({}));
    for (name, kind) in [
        ("clip.mp4", skwd_wall_core::infrastructure::paper::SourceKind::Video),
        ("still.png", skwd_wall_core::infrastructure::paper::SourceKind::Static),
    ] {
        let path = root.path().join(name);
        std::fs::write(&path, []).unwrap();
        let source = super::media_source(&config, path.to_str().unwrap()).unwrap();
        assert_eq!(source.kind, kind);
        assert_eq!(source.path, path.to_str().unwrap());
    }
    assert_eq!(
        super::media_source(&config, root.path().to_str().unwrap()).unwrap().kind,
        skwd_wall_core::infrastructure::paper::SourceKind::WallpaperEngine
    );
    assert!(super::media_source(&config, "we:../escape").is_err());
    assert!(super::media_source(&config, "/missing/backdrop.mp4").is_err());
}

#[test]
fn legacy_cleanup_stays_in_its_wayland_session() {
    let environment = b"WAYLAND_DISPLAY=wayland-1\0XDG_RUNTIME_DIR=/run/user/1000\0";
    assert!(super::legacy_session_matches(environment, "wayland-1", "/run/user/1000"));
    assert!(!super::legacy_session_matches(environment, "wayland-2", "/run/user/1000"));
    assert!(!super::legacy_session_matches(environment, "wayland-1", "/tmp/private-runtime"));
}

#[test]
fn stationary_preserves_main_service() {
    use skwd_wall_core::infrastructure::paper::{PaperClient, Startup};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    if std::env::var_os("SKWD_OVERVIEW_TEST").is_none() {
        assert!(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "infrastructure::overview_backdrop::tests::stationary_preserves_main_service",
                    "--nocapture"
                ])
                .env("SKWD_OVERVIEW_TEST", "1")
                .env("NIRI_SOCKET", "fixture")
                .env("XDG_CURRENT_DESKTOP", "niri")
                .env("SKWD_FAKE_OUTPUTS", "DP-1")
                .status()
                .unwrap()
                .success()
        );
        return;
    }

    let (_guard, root) = crate::testenv::lock();
    let source = root.join("walls/still.png");
    image::RgbImage::new(2, 2).save(&source).unwrap();
    let assignment = paper_control::Assignment::new(
        vec!["DP-1".into()],
        paper_control::Source::static_file(source.display().to_string()),
    );
    let sockets = tempfile::tempdir().unwrap();
    let socket = sockets.path().join("main.sock");
    unsafe { std::env::set_var("SKWD_PAPER_V2_SOCKET", &socket) };
    let finished = Arc::new(AtomicBool::new(false));
    let main = surface_server(&socket, vec![assignment.clone()], Arc::clone(&finished));
    let backdrop_socket = socket.with_file_name("overview-backdrop.sock");
    let backdrop = surface_server(&backdrop_socket, Vec::new(), Arc::clone(&finished));
    crate::testenv::write_config(serde_json::json!({
        "niri":{"overviewBackdrop":true,"overviewMode":"stationary",
            "backdropBlurStatic":true,"backdropBlurStaticRadius":11,
            "backdropBlurVideo":true,"backdropBlurVideoRadius":22,
            "backdropBlurWe":true,"backdropBlurWeRadius":33,"backdropDim":44}
    }));
    let state = Arc::new(skwd_wall_core::WallState::open().unwrap());
    let local_frames = root.join("local-surface.ndjson");
    let mut local = std::process::Command::new("cat")
        .stdin(std::process::Stdio::piped())
        .stdout(std::fs::File::create(&local_frames).unwrap())
        .spawn()
        .unwrap();
    let local_stdin = local.stdin.take();
    state.renderers().set_output_still("DP-2", local, local_stdin);
    let desired =
        serde_json::json!({"DP-1":{"type":"static","path":source,"mute":true,"volume":100}});
    skwd_wall_core::audio::write_state(&state.config().cache_dir(), &desired);
    super::refresh_from_disk(&state).unwrap();
    let client = PaperClient::configured(&state.config());
    let status = client.status(Startup::ExistingOnly).unwrap();
    assert_eq!(status.assignments[0].source, assignment.source);
    assert_eq!(status.policy.unwrap().surface.unwrap().namespace, "skwd-paper-stationary");
    super::set_overview(Some(true));
    super::on_apply(&state);
    super::set_overview(Some(false));
    crate::testenv::write_config(serde_json::json!({"niri":{"overviewBackdrop":false}}));
    state.reload_config();
    super::refresh_from_disk(&state).unwrap();
    assert_eq!(
        client.status(Startup::ExistingOnly).unwrap().assignments[0].source,
        assignment.source
    );
    assert_eq!(skwd_wall_core::audio::read_state(&state.config().cache_dir()), desired);
    finished.store(true, Ordering::Release);
    client.status(Startup::ExistingOnly).unwrap();
    PaperClient::new("unused", backdrop_socket).status(Startup::ExistingOnly).unwrap();
    let main = main.join().unwrap();
    let backdrop = backdrop.join().unwrap();
    check_surface_requests(&main, &backdrop);
    let (mut local, stdin) = state.renderers().take_output_still("DP-2").unwrap();
    drop(stdin);
    assert!(local.wait().unwrap().success());
    let commands = std::fs::read_to_string(local_frames).unwrap();
    let blur = commands
        .lines()
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line).unwrap()["surface"]["blur"]
                .as_u64()
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(blur, [0, 11, 11, 0]);
}

fn check_surface_requests(
    main: &[paper_control::RequestParams],
    backdrop: &[paper_control::RequestParams],
) {
    let surfaces = main
        .iter()
        .filter_map(|request| match request {
            paper_control::RequestParams::SurfaceOverrides(request) => Some(&request.surfaces),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(surfaces.first().unwrap().is_empty());
    assert!(surfaces.last().unwrap().is_empty());
    assert_eq!(surfaces.len(), 6);
    for (kind, blur) in [
        (paper_control::SourceKind::Static, 11),
        (paper_control::SourceKind::Video, 22),
        (paper_control::SourceKind::WallpaperEngine, 33),
    ] {
        assert_eq!(surfaces[1][&kind].blur, 0);
        assert_eq!(surfaces[2][&kind].blur, blur);
        assert_eq!(surfaces[2][&kind].dim, 44);
        assert_eq!(surfaces[3][&kind], surfaces[2][&kind]);
        assert_eq!(surfaces[4][&kind].blur, 0);
        assert_eq!(surfaces[4][&kind].dim, 0);
    }
    assert_eq!(
        main.iter()
            .filter(|request| matches!(request, paper_control::RequestParams::Apply(_)))
            .count(),
        2
    );
    assert!(!main.iter().any(|request| matches!(request, paper_control::RequestParams::Stop(_))));
    assert_eq!(
        backdrop
            .iter()
            .filter(|request| matches!(request, paper_control::RequestParams::Stop(_)))
            .count(),
        2
    );
}

fn surface_server(
    socket: &std::path::Path,
    assignments: Vec<paper_control::Assignment>,
    finished: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> std::thread::JoinHandle<Vec<paper_control::RequestParams>> {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::os::unix::net::UnixListener::bind(socket).unwrap();
    std::thread::spawn(move || {
        let mut status = paper_control::StatusResult {
            paused: false,
            policy: None,
            renderers: Vec::new(),
            assignments: assignments
                .iter()
                .map(|assignment| {
                    paper_control::AssignmentStatus::from_assignment(assignment, 1, Some(17), true)
                })
                .collect(),
        };
        let mut requests = Vec::new();
        let mut surfaces = std::collections::BTreeMap::new();
        loop {
            let (mut stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(stream.try_clone().unwrap()).read_line(&mut line).unwrap();
            let request: paper_control::Request = paper_control::decode_ndjson(&line).unwrap();
            let result = match &request.params {
                paper_control::RequestParams::Status(_) => serde_json::to_value(&status).unwrap(),
                paper_control::RequestParams::SurfaceOverrides(surface) => {
                    surfaces.clone_from(&surface.surfaces);
                    serde_json::to_value(surface).unwrap()
                }
                paper_control::RequestParams::Apply(apply) => {
                    assert!(surfaces.is_empty());
                    status.policy.clone_from(&apply.policy);
                    status.assignments = apply
                        .assignments
                        .iter()
                        .map(|assignment| {
                            paper_control::AssignmentStatus::from_assignment(
                                assignment,
                                2,
                                Some(18),
                                true,
                            )
                        })
                        .collect();
                    serde_json::json!({"generation":2,"assignments":[]})
                }
                paper_control::RequestParams::Stop(_) => {
                    status.assignments.clear();
                    serde_json::json!({"stopped":1})
                }
                paper_control::RequestParams::Capabilities(_) => {
                    serde_json::to_value(paper_control::CapabilitiesResult::current()).unwrap()
                }
                paper_control::RequestParams::PlaybackPause(pause) => {
                    serde_json::json!({"paused":pause.paused})
                }
                paper_control::RequestParams::PlaybackAudio(_) => {
                    serde_json::json!({"updated":0,"assignments":[]})
                }
                _ => panic!("unexpected Paper request"),
            };
            requests.push(request.params);
            writeln!(stream, "{}", serde_json::json!({"id":request.id,"result":result})).unwrap();
            if finished.load(std::sync::atomic::Ordering::Acquire) {
                return requests;
            }
        }
    })
}
