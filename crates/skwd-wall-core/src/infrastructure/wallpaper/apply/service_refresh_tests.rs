use super::*;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;

#[test]
fn service_refresh_owners() {
    if let Ok(case) = std::env::var("SKWD_SERVICE_REFRESH_CASE") {
        check_refresh(&case);
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let outputs = temp.path().join("outputs");
    std::fs::write(&outputs, "DP-1:1920x1080,DP-2:1920x1080,DP-3:1920x1080").unwrap();
    for case in ["scene", "global", "retry", "local-retry", "drawing-cleanup", "drawing-surfaces"] {
        let socket = temp.path().join(case);
        assert!(Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "infrastructure::wallpaper::apply::tests::service_refresh::service_refresh_owners", "--nocapture"])
            .env("SKWD_SERVICE_REFRESH_CASE", case)
            .env("NIRI_SOCKET", "fixture")
            .env("SKWD_PAPER_V2_SOCKET", socket)
            .env("SKWD_FAKE_OUTPUTS_FILE", &outputs)
            .status().unwrap().success(), "{case}");
    }
}

fn check_refresh(case: &str) {
    if case.starts_with("drawing-") {
        check_drawing(case);
        return;
    }
    let mut st =
        Stub::with_display(&serde_json::json!({"fillMode":"fit","outputLocks":{"DP-2":true}}));
    let mut config = st.config().root().clone();
    config["weRender"] = serde_json::json!({"fps":24});
    st.state = Arc::new(WallState::test_new(config));
    seed(&st, "DP-1", "we", "", "42", false, 61);
    seed(&st, "DP-2", "static", "/w/locked.png", "", true, 50);
    seed(&st, "DP-3", "video", "/v/local.mp4", "", false, 72);
    let still = capture_child(&st.path().join("local-still"));
    let still_pid = still.0.id();
    st.renderers().restore_output_still("DP-2", still);
    let video = capture_child(&st.path().join("local-video"));
    let video_pid = video.0.id();
    st.renderers().set_video_paper("multi", video.0, video.1);
    st.renderers().set_assignment("multi", "DP-3=/v/local.mp4");
    st.renderers().set_assignment("DP-1", "/we/42");
    record_paper_policy(&st);
    if case != "scene" {
        st.renderers().set_policy(PAPER_POLICY_KEY, "old-policy");
    }
    let signature = st.renderers().policy(PAPER_POLICY_KEY);
    let desired = recorded(&st);
    let replies = match case {
        "retry" => vec![false, true],
        "local-retry" => vec![true, true],
        _ => vec![true],
    };
    let server = paper_server(Arc::clone(&st.state), replies);
    let binary = st.config().renderer().vk_bin();
    let saved = st.path().join("saved-renderer");
    if case == "local-retry" {
        std::fs::rename(&binary, &saved).unwrap();
    }
    if case.ends_with("retry") {
        assert!(refresh_policy_ready(&st).is_err());
        assert_eq!(st.renderers().policy(PAPER_POLICY_KEY), signature);
        assert_eq!(st.renderers().video_paper_pid("multi"), Some(video_pid));
        assert!(st.renderers().output_still_pid_alive("DP-2", still_pid));
    }
    if case == "local-retry" {
        std::fs::rename(&saved, &binary).unwrap();
    }
    refresh_policy_ready(&st).unwrap();
    let requests = server.join().unwrap();
    assert_eq!(recorded(&st), desired);
    assert!(paper_policy_matches(&st));
    assert!(!st.renderers().has_video_paper("DP-1"));
    assert_eq!(st.renderers().assignments()["DP-1"], "/we/42");
    if case == "scene" {
        assert_eq!(st.renderers().video_paper_pid("multi"), Some(video_pid));
        assert!(st.renderers().output_still_pid_alive("DP-2", still_pid));
    } else {
        assert!(!st.renderers().wallpaper_pids().contains(&video_pid));
        assert!(!st.renderers().wallpaper_pids().contains(&still_pid));
    }
    for request in requests {
        assert!(request.replace_all);
        assert_eq!(request.assignments.len(), 1);
        let scene = &request.assignments[0];
        assert_eq!(scene.outputs, ["DP-1"]);
        assert_eq!(scene.source.properties.as_ref().unwrap()["speed"], 0.7);
        assert_eq!(scene.fill_mode, paper_control::FillMode::Fit);
        assert_eq!(scene.volume, 61);
        assert!(scene.transition.is_none());
        assert_eq!(request.policy.unwrap().scene.unwrap().fps, Some(24));
    }
}

fn paper_server(
    st: Arc<WallState>,
    replies: Vec<bool>,
) -> std::thread::JoinHandle<Vec<paper_control::ApplyRequest>> {
    let listener = UnixListener::bind(crate::infrastructure::paper::paper_socket_path()).unwrap();
    let scene = paper_control::Assignment::new(
        vec!["DP-1".into()],
        paper_control::Source::wallpaper_engine(
            st.config().we_dir().join("42").display().to_string(),
        )
        .with_properties(serde_json::from_value(serde_json::json!({"speed":0.7})).unwrap()),
    );
    let mut status = paper_control::StatusResult {
        paused: false,
        policy: Some(crate::infrastructure::paper::renderer_policy(
            &st.config(),
            &crate::outputs::enumerate(),
        )),
        assignments: vec![paper_control::AssignmentStatus::from_assignment(
            &scene,
            1,
            Some(17),
            true,
        )],
        renderers: Vec::new(),
    };
    std::thread::spawn(move || {
        let mut requests = Vec::new();
        let cleanup =
            std::env::var("SKWD_SERVICE_REFRESH_CASE").as_deref() == Ok("drawing-cleanup");
        let seeded =
            std::env::var("SKWD_SERVICE_REFRESH_CASE").as_deref() == Ok("drawing-surfaces");
        let mut surfaces = None;
        if seeded {
            status.assignments.clear();
        }
        loop {
            let (mut stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(stream.try_clone().unwrap()).read_line(&mut line).unwrap();
            let request: paper_control::Request = paper_control::decode_ndjson(&line).unwrap();
            let audio = matches!(&request.params, paper_control::RequestParams::PlaybackAudio(_));
            let response = match request.params {
                paper_control::RequestParams::Status(_) => {
                    serde_json::json!({"id":request.id,"result":status})
                }
                paper_control::RequestParams::SurfaceOverrides(requested) => {
                    assert!(requests.is_empty());
                    surfaces = Some(requested.clone());
                    serde_json::json!({"id":request.id,"result":requested})
                }
                paper_control::RequestParams::Apply(apply) => {
                    if seeded {
                        let surfaces: &paper_control::SurfaceOverridesRequest =
                            surfaces.as_ref().unwrap();
                        assert_eq!(surfaces.surfaces.len(), 3);
                        for (kind, blur) in [
                            (paper_control::SourceKind::Static, 11),
                            (paper_control::SourceKind::Video, 22),
                            (paper_control::SourceKind::WallpaperEngine, 33),
                        ] {
                            assert_eq!(surfaces.surfaces[&kind].blur, blur);
                            assert_eq!(surfaces.surfaces[&kind].dim, 44);
                        }
                    }
                    if cleanup {
                        let (child, stdin) = st.renderers().take_video_paper("DP-1,DP-3").unwrap();
                        drop(stdin);
                        st.renderers().set_video_paper("DP-1,DP-3", child, None);
                    }
                    let result = serde_json::json!({"generation":2,"assignments":[]});
                    let accepted = replies[requests.len()];
                    requests.push(apply);
                    if accepted {
                        serde_json::json!({"id":request.id,"result":result})
                    } else {
                        serde_json::json!({"id":request.id,"error":{"code":"rejected","message":"fixture rejection"}})
                    }
                }
                paper_control::RequestParams::Capabilities(_) => {
                    serde_json::json!({"id":request.id,"result":paper_control::CapabilitiesResult::current()})
                }
                paper_control::RequestParams::PlaybackPause(pause) => {
                    serde_json::json!({"id":request.id,"result":{"paused":pause.paused}})
                }
                paper_control::RequestParams::PlaybackAudio(_) => {
                    serde_json::json!({"id":request.id,"result":{"updated":0,"assignments":[]}})
                }
                _ => panic!("unexpected Paper request"),
            };
            writeln!(stream, "{response}").unwrap();
            if (audio || cleanup || seeded) && requests.len() == replies.len() {
                return requests;
            }
        }
    })
}

fn check_drawing(case: &str) {
    use crate::infrastructure::drawing as cache;
    let mut st = Stub::new();
    if case == "drawing-surfaces" {
        let mut config = st.config().root().clone();
        config["niri"] = serde_json::json!({
            "overviewBackdrop":true,"overviewMode":"stationary",
            "backdropBlurStatic":true,"backdropBlurStaticRadius":11,
            "backdropBlurVideo":true,"backdropBlurVideoRadius":22,
            "backdropBlurWe":true,"backdropBlurWeRadius":33,"backdropDim":44
        });
        st.state = Arc::new(WallState::test_new(config));
        crate::infrastructure::paper::set_stationary_overview(true);
    }
    let source = st.path().join("next.png");
    std::fs::write(&source, b"next wallpaper").unwrap();
    let entry = cache::CacheEntry {
        artifact: cache::digest(b"cleanup artifact"),
        pack: cache::digest(b"pack"),
        recipe: cache::RECIPE.into(),
    };
    let artifact = cache::artifacts(&st.config()).join(&entry.artifact);
    std::fs::create_dir_all(&artifact).unwrap();
    std::fs::write(artifact.join("drawing.png"), b"texture").unwrap();
    std::fs::write(artifact.join("strokes.bin"), b"strokes").unwrap();
    let metadata = source.metadata().unwrap();
    let manifest = serde_json::json!({
        "version":3,"source":{"path":source,"size":metadata.len(),
        "mtime_ns":metadata.modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),"sha256":cache::hash_file(&source).unwrap()},
        "width":10,"height":10,"texture":"drawing.png","strokes":"strokes.bin","timing_width":8,"timing_height":8,"focus":[0.5,0.5]
    });
    std::fs::write(artifact.join(cache::MANIFEST), serde_json::to_vec(&manifest).unwrap()).unwrap();
    cache::publish(&st.config(), &source, &entry).unwrap();
    seed(&st, "DP-1", "we", "", "42", false, 61);
    seed(&st, "DP-3", "we", "", "42", false, 61);
    let previous = recorded(&st);
    let mut child = Command::new("python3")
        .args([
            "-u",
            "-c",
            r#"import json, pathlib, sys
for line in sys.stdin:
    snapshot = json.loads(line).get("snapshot")
    if snapshot:
        pathlib.Path(snapshot["path"]).write_bytes(b"P6\n2 2\n255\n" + b"\xff\0\0" * 4)
"#,
        ])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let stdin = child.stdin.take();
    let pid = child.id();
    st.renderers().set_video_paper("DP-1,DP-3", child, stdin);
    st.renderers().mark_scene_paper("DP-1,DP-3", true);
    st.renderers().set_assignment("DP-1", "/we/42");
    let server = paper_server(Arc::clone(&st.state), vec![true]);
    crate::infrastructure::wallpaper::preview(&st.state, "static:next.png", &source, "DP-1")
        .unwrap();
    assert_eq!(server.join().unwrap().len(), 1);
    let committed = recorded(&st);
    assert_eq!(committed["DP-1"]["path"], source.to_str().unwrap());
    assert_eq!(committed["DP-3"], previous["DP-3"]);
    assert_eq!(st.renderers().assignments()["DP-1"], source.to_str().unwrap());
    let group = if case == "drawing-cleanup" { "DP-1,DP-3" } else { "DP-3" };
    assert_eq!(st.renderers().video_paper_pid(group), Some(pid));
}
