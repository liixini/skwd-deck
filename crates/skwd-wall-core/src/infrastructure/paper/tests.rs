use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;

use serde_json::json;

use paper_control::{Request, RequestParams, Response, ResponseBody, ResponseError};

use super::adapter::{assignment_with_options, video_engine};
use super::*;
use crate::backend::wallpaper::ApplyOutputRequest;
use crate::config::Config;

fn assignment(outputs: &[&str], source: Source) -> Assignment {
    Assignment::new(outputs.iter().map(|output| (*output).to_string()).collect(), source)
}

#[test]
fn mixed_apply_golden() {
    let request = Request::new(
        7,
        RequestParams::Apply(ApplyRequest {
            assignments: vec![
                assignment(&["DP-1"], Source::static_file("/wall/a.png")),
                assignment(&["DP-2"], Source::video("/wall/b.mp4", Some(VideoEngine::Default))),
                assignment(&["HDMI-A-1"], Source::wallpaper_engine("/scene/forest")),
            ],
            replace_all: true,
            policy: None,
        }),
    );
    let golden = include_str!("../../../tests/golden/paper-apply-mixed-v1.jsonl");
    assert_eq!(encode_ndjson(&request).unwrap(), golden);
    assert_eq!(decode_ndjson::<Request>(golden).unwrap(), request);
}

#[test]
fn video_engines_golden() {
    let request = Request::new(
        13,
        RequestParams::Apply(ApplyRequest {
            assignments: vec![
                assignment(
                    &["DP-1"],
                    Source::video("/wall/default.mp4", Some(VideoEngine::Default)),
                ),
                assignment(&["DP-3"], Source::tinier_video("/wall/tinier.ivf", "30000/1001")),
            ],
            replace_all: false,
            policy: None,
        }),
    );
    let golden = include_str!("../../../tests/golden/paper-apply-video-engines-v1.jsonl");
    assert_eq!(encode_ndjson(&request).unwrap(), golden);
    assert_eq!(decode_ndjson::<Request>(golden).unwrap(), request);
}

#[test]
fn assignment_options_golden() {
    let configured = assignment_with_options(
        vec!["DP-1".into()],
        Source {
            kind: SourceKind::Video,
            path: "/wall/a.mp4".into(),
            engine: None,
            frame_rate: None,
            properties: None,
        },
        FillMode::Fit,
        false,
        55,
        Layer::Top,
    );
    let request = Request::new(
        14,
        RequestParams::Apply(ApplyRequest {
            assignments: vec![configured],
            replace_all: false,
            policy: None,
        }),
    );
    let golden = include_str!("../../../tests/golden/paper-apply-options-v1.jsonl");
    assert_eq!(encode_ndjson(&request).unwrap(), golden);
    assert_eq!(decode_ndjson::<Request>(golden).unwrap(), request);
}

#[test]
fn transition_policy_golden() {
    let mut configured = assignment(
        &["DP-1"],
        Source {
            kind: SourceKind::Video,
            path: "/wall/b.mp4".into(),
            engine: None,
            frame_rate: None,
            properties: None,
        },
    );
    configured.transition = Some(TransitionPolicy {
        from: Some("/wall/a.png".into()),
        effect: Some("sand-bloom".into()),
        duration_ms: Some(700),
    });
    let policy = RendererPolicy {
        idle_seconds: Some(45),
        transitions_enabled: Some(true),
        sand: Some(SandPolicy {
            quality: Some(SandQuality::Low),
            scope: Some(SandScope::Primary),
            primary: Some("DP-1".into()),
            sharp: Some(true),
            fps: Some(30),
        }),
        scene: Some(ScenePolicy {
            fps: Some(60),
            disable_particles: Some(true),
            assets_dir: Some("/we/assets".into()),
            max_dimension: Some(2048),
            max_effect_chains: Some(4),
            max_effect_passes: Some(8),
            strict: Some(true),
        }),
        output_fps: [("DP-1".into(), 60), ("DP-2".into(), 120)].into(),
    };
    let request = Request::new(
        15,
        RequestParams::Apply(ApplyRequest {
            assignments: vec![configured],
            replace_all: true,
            policy: Some(policy),
        }),
    );
    let golden = include_str!("../../../tests/golden/paper-apply-policy-v1.jsonl");
    assert_eq!(encode_ndjson(&request).unwrap(), golden);
    assert_eq!(decode_ndjson::<Request>(golden).unwrap(), request);
}

#[test]
fn response_goldens() {
    let source = Source {
        kind: SourceKind::Video,
        path: "/wall/a.mp4".into(),
        engine: None,
        frame_rate: None,
        properties: None,
    };
    let status = AssignmentStatus {
        outputs: vec!["DP-1".into()],
        source,
        fill_mode: FillMode::Fill,
        mute: true,
        volume: 80,
        layer: Layer::Background,
        generation: 44,
        pid: Some(321),
        ready: true,
    };
    let apply = Response {
        id: 7,
        body: ResponseBody::Success {
            result: ApplyResult {
                generation: 44,
                paused: false,
                policy: None,
                assignments: vec![status],
            },
        },
    };
    let apply_golden = include_str!("../../../tests/golden/paper-apply-response-v1.jsonl");
    assert_eq!(encode_ndjson(&apply).unwrap(), apply_golden);
    assert_eq!(decode_ndjson::<Response<ApplyResult>>(apply_golden).unwrap(), apply);

    let capabilities =
        Response { id: 10, body: ResponseBody::Success { result: CapabilitiesResult::current() } };
    let capabilities_golden =
        include_str!("../../../tests/golden/paper-capabilities-response-v1.jsonl");
    assert_eq!(encode_ndjson(&capabilities).unwrap(), capabilities_golden);
    assert_eq!(
        decode_ndjson::<Response<CapabilitiesResult>>(capabilities_golden).unwrap(),
        capabilities
    );
}

#[test]
fn phase_one_response_defaults() {
    let apply = decode_ndjson::<Response<ApplyResult>>(
        "{\"id\":7,\"result\":{\"generation\":44,\"assignments\":[]}}\n",
    )
    .unwrap();
    let ResponseBody::Success { result } = apply.body else {
        panic!("expected Paper apply result");
    };
    assert!(!result.paused);
    assert_eq!(result.policy, None);

    let capabilities = decode_ndjson::<Response<CapabilitiesResult>>(concat!(
        r#"{"id":10,"result":{"protocol":"skwd-paper","version":1,"#,
        r#""source_kinds":[],"video_engines":[],"fill_modes":[],"layers":[]}}"#,
        "\n"
    ))
    .unwrap();
    let ResponseBody::Success { result } = capabilities.body else {
        panic!("expected Paper capabilities result");
    };
    assert_eq!(result.controls, ControlCapabilities::default());
    assert_eq!(result.transitions, TransitionCapabilities::default());
    assert_eq!(result.renderer_policy, RendererPolicyCapabilities::default());
}

#[test]
fn video_engine_mapping() {
    assert_eq!(video_engine("tinier"), VideoEngine::Tinier);
    for value in ["", "vulkan", "regular", "tiny", "gl", "anything"] {
        assert_eq!(video_engine(value), VideoEngine::Default, "{value}");
    }
}

#[test]
fn adapter_tinier_source() {
    let config = Config::from_root(json!({"paper": {"videoEngine": "tinier"}}));
    let adapter =
        PaperClientAdapter::new(PaperClient::new("unused-paper", "/unused/paper.sock"), config);
    let assignment = adapter
        .assignment(ApplyOutputRequest {
            output: "DP-1",
            kind: wall_proto::kind::VIDEO,
            path: "/cache/video.tinier-v1.ivf",
            we_id: "",
            fill_mode: "fill",
            mute: true,
            volume: 0,
            frame_rate: Some("30000/1001"),
            transition: None,
        })
        .unwrap();
    assert_eq!(assignment.source.engine, Some(VideoEngine::Tinier));
    assert_eq!(assignment.source.frame_rate.as_deref(), Some("30000/1001"));
}

#[test]
fn client_apply_round_trip() {
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("paper.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(stream.try_clone().unwrap()).read_line(&mut line).unwrap();
        let request: Request = decode_ndjson(&line).unwrap();
        let RequestParams::Apply(apply) = request.params else {
            panic!("expected Paper apply");
        };
        assert_eq!(apply.assignments[0].source.kind, SourceKind::Video);
        let source = apply.assignments[0].source.clone();
        let response = Response {
            id: request.id,
            body: ResponseBody::Success {
                result: ApplyResult {
                    generation: 41,
                    paused: false,
                    policy: None,
                    assignments: vec![AssignmentStatus {
                        outputs: vec!["DP-1".into()],
                        source,
                        fill_mode: FillMode::Fill,
                        mute: true,
                        volume: 80,
                        layer: Layer::Background,
                        generation: 41,
                        pid: Some(700),
                        ready: true,
                    }],
                },
            },
        };
        stream.write_all(encode_ndjson(&response).unwrap().as_bytes()).unwrap();
    });
    let client = PaperClient::new("unused-paper", &socket);
    let result = client
        .apply(ApplyRequest {
            assignments: vec![assignment(
                &["DP-1"],
                Source::video("/wall/a.mp4", Some(VideoEngine::Default)),
            )],
            replace_all: false,
            policy: None,
        })
        .unwrap();
    assert_eq!(result.generation, 41);
    assert!(result.assignments[0].ready);
    server.join().unwrap();
}

#[test]
fn adapter_retired_tiny_engine() {
    let config = Config::from_root(json!({"paper": {"videoEngine": "tiny"}}));
    let adapter =
        PaperClientAdapter::new(PaperClient::new("unused-paper", "/unused/paper.sock"), config);
    let assignment = adapter
        .assignment(ApplyOutputRequest {
            output: "DP-1",
            kind: wall_proto::kind::VIDEO,
            path: "/wall/a.mp4",
            we_id: "",
            fill_mode: "fit",
            mute: false,
            volume: 55,
            frame_rate: None,
            transition: None,
        })
        .unwrap();
    assert_eq!(assignment.source.engine, Some(VideoEngine::Default));
    assert_eq!(assignment.fill_mode, FillMode::Fit);
    assert!(!assignment.mute);
    assert_eq!(assignment.volume, 55);
}

#[test]
fn invalid_apply_rejected() {
    let client = PaperClient::new("unused-paper", "/unused/paper.sock");
    let error = client
        .apply(ApplyRequest { assignments: Vec::new(), replace_all: false, policy: None })
        .unwrap_err();
    assert!(error.to_string().contains("at least one assignment"));
}

#[test]
fn invalid_stop_rejected() {
    let client = PaperClient::new("unused-paper", "/unused/paper.sock");
    let error = client.stop(vec!["*".into(), "DP-1".into()]).unwrap_err();
    assert!(error.to_string().contains("wildcard output"));
}

#[test]
fn oversized_request_rejected() {
    let client = PaperClient::new("unused-paper", "/unused/paper.sock");
    let request = ApplyRequest {
        assignments: vec![assignment(
            &["DP-1"],
            Source::static_file(format!("/{}", "a".repeat(1024 * 1024))),
        )],
        replace_all: false,
        policy: None,
    };
    let error = client.apply(request).unwrap_err();
    assert!(error.to_string().contains("request exceeds 1048576 bytes"));
}

fn composition_adapter() -> PaperClientAdapter {
    PaperClientAdapter::new(
        PaperClient::new("unused-paper", "/unused/paper.sock"),
        Config::from_root(json!({})),
    )
}

fn output(name: &str, refresh_mhz: i32) -> crate::outputs::OutputInfo {
    crate::outputs::OutputInfo {
        name: name.into(),
        refresh_mhz,
        ..crate::outputs::OutputInfo::default()
    }
}

fn replacement(plan: PaperCompositionPlan) -> ApplyRequest {
    match plan {
        PaperCompositionPlan::Replace(request) => request,
        PaperCompositionPlan::StopAll => panic!("expected replacement plan"),
    }
}

#[test]
fn composition_shared_tiny() {
    let adapter = composition_adapter();
    let config = Config::from_root(json!({"paper": {"videoEngine": "tiny"}}));
    let state = json!({
        "*": crate::audio::entry("video", "/video/a.mp4", "", false, 45)
    });
    let request = replacement(
        adapter
            .composition_plan(&config, &state, &[output("DP-2", 144_000), output("DP-1", 60_000)])
            .unwrap(),
    );
    assert!(request.replace_all);
    assert_eq!(request.assignments.len(), 1);
    assert_eq!(request.assignments[0].outputs, ["*"]);
    assert_eq!(request.assignments[0].source.engine, Some(VideoEngine::Default));
    assert!(!request.assignments[0].mute);
    assert_eq!(request.policy.unwrap().output_fps.len(), 2);
}

#[test]
fn composition_wildcard_divergent_fills() {
    let adapter = composition_adapter();
    let config = Config::from_root(json!({
        "display": {"fillMode": "fill", "fillModes": {"DP-2": "fit"}}
    }));
    let state = json!({
        "*": crate::audio::entry("static", "/wall/a.png", "", true, 80)
    });
    let request = replacement(
        adapter
            .composition_plan(&config, &state, &[output("DP-2", 144_000), output("DP-1", 60_000)])
            .unwrap(),
    );
    assert_eq!(request.assignments.len(), 2);
    assert_eq!(request.assignments[0].outputs, ["DP-1"]);
    assert_eq!(request.assignments[0].fill_mode, FillMode::Fill);
    assert_eq!(request.assignments[1].outputs, ["DP-2"]);
    assert_eq!(request.assignments[1].fill_mode, FillMode::Fit);
}

#[test]
fn composition_wildcard_override_dedup() {
    let adapter = composition_adapter();
    let config = Config::from_root(json!({
        "display": {"fillMode": "fill", "fillModes": {"DP-2": "fit"}}
    }));
    let entry = crate::audio::entry("video", "/video/a.mp4", "", false, 45);
    let state = json!({"*": entry.clone(), "DP-2": entry});
    let request = replacement(
        adapter
            .composition_plan(&config, &state, &[output("DP-2", 144_000), output("DP-1", 60_000)])
            .unwrap(),
    );
    assert_eq!(request.assignments.len(), 2);
    assert_eq!(request.assignments[0].outputs, ["DP-1"]);
    assert!(!request.assignments[0].mute);
    assert_eq!(request.assignments[1].outputs, ["DP-2"]);
    assert_eq!(request.assignments[1].fill_mode, FillMode::Fit);
    assert!(request.assignments[1].mute);
}

#[test]
fn composition_mixed_kinds() {
    let adapter = composition_adapter();
    let config = Config::from_root(json!({
        "paths": {"steamWorkshop": "/workshop"},
        "paper": {"videoEngine": "regular"},
        "display": {"fillModes": {"DP-2": "center", "HDMI-A-1": "span"}}
    }));
    let state = json!({
        "DP-1": crate::audio::entry("static", "/wall/a.png", "", true, 80),
        "DP-2": crate::audio::entry("video", "/video/b.mp4", "", false, 55),
        "HDMI-A-1": crate::audio::entry("we", "", "431960", false, 37)
    });
    let request = replacement(
        adapter
            .composition_plan(
                &config,
                &state,
                &[output("HDMI-A-1", 60_000), output("DP-2", 144_000), output("DP-1", 60_000)],
            )
            .unwrap(),
    );
    assert_eq!(request.assignments.len(), 3);
    assert_eq!(request.assignments[0].source.kind, SourceKind::Static);
    assert_eq!(request.assignments[1].source.kind, SourceKind::Video);
    assert_eq!(request.assignments[1].source.engine, Some(VideoEngine::Default));
    assert_eq!(request.assignments[1].fill_mode, FillMode::Center);
    assert_eq!(request.assignments[1].volume, 55);
    assert_eq!(request.assignments[2].source.kind, SourceKind::WallpaperEngine);
    assert_eq!(request.assignments[2].source.path, "/workshop/431960");
    assert_eq!(request.assignments[2].fill_mode, FillMode::Span);
}

#[test]
fn tinier_composition_artifact() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source.mp4");
    let artifact = temp.path().join("loop.tinier-v1.ivf");
    std::fs::write(&source, b"original-video-bytes").unwrap();
    std::fs::write(&artifact, b"validated-av1").unwrap();
    let wall = crate::state::WallState::test_new(json!({"paper": {"videoEngine": "tinier"}}));
    wall.with_db(|connection| {
        crate::db::tinier_convert_record(
            connection,
            source.to_str().unwrap(),
            artifact.to_str().unwrap(),
            "30000/1001",
            crate::db::TINIER_CONVERT_PRESET,
            20,
            13,
        )
    })
    .unwrap();
    let state = json!({
        "DP-1": crate::audio::entry("static", "/wall/a.png", "", true, 0),
        "DP-2": crate::audio::entry("video", artifact.to_str().unwrap(), "", false, 75)
    });
    let request = replacement(
        composition_adapter()
            .tinier_composition_plan(
                &wall,
                &state,
                &[output("DP-1", 60_000), output("DP-2", 60_000)],
            )
            .unwrap(),
    );
    assert!(request.replace_all);
    assert_eq!(request.assignments[0].source.kind, SourceKind::Static);
    assert_eq!(request.assignments[1].source.engine, Some(VideoEngine::Tinier));
    assert_eq!(request.assignments[1].source.frame_rate.as_deref(), Some("30000/1001"));
    assert!(request.assignments[1].mute);
}

#[test]
fn composition_hotplug() {
    let adapter = composition_adapter();
    let config = Config::from_root(json!({}));
    let state = json!({
        "DP-1": crate::audio::entry("static", "/wall/a.png", "", true, 80),
        "DP-2": crate::audio::entry("video", "/video/b.mp4", "", false, 60)
    });
    let initial =
        replacement(adapter.composition_plan(&config, &state, &[output("DP-1", 60_000)]).unwrap());
    assert_eq!(initial.assignments[0].outputs, ["DP-1"]);
    let hotplugged = replacement(
        adapter
            .composition_plan(&config, &state, &[output("DP-1", 60_000), output("DP-2", 144_000)])
            .unwrap(),
    );
    assert_eq!(hotplugged.assignments.len(), 2);
    let removed =
        replacement(adapter.composition_plan(&config, &state, &[output("DP-2", 144_000)]).unwrap());
    assert_eq!(removed.assignments.len(), 1);
    assert_eq!(removed.assignments[0].outputs, ["DP-2"]);
}

#[test]
fn composition_renderer_policy() {
    let config = Config::from_root(json!({
        "paper": {"idlePauseSeconds": 17, "performanceMode": true},
        "transition": {
            "shader": "sand-donut",
            "sandFps": 72,
            "sandPrimary": "DP-2",
            "sandQuality": "full",
            "sandScope": "primary",
            "sandSharp": true
        },
        "paths": {"steamWeAssets": "/we-assets"},
        "weRender": {"fps": 144, "disableParticles": true}
    }));
    let policy = renderer_policy(&config, &[output("DP-1", 59_997), output("DP-2", 144_001)]);
    assert_eq!(policy.idle_seconds, Some(17));
    assert_eq!(policy.transitions_enabled, Some(false));
    assert_eq!(policy.output_fps.get("DP-1"), Some(&60));
    assert_eq!(policy.output_fps.get("DP-2"), Some(&144));
    let sand = policy.sand.unwrap();
    assert_eq!(sand.quality, Some(SandQuality::Full));
    assert_eq!(sand.scope, Some(SandScope::Primary));
    assert_eq!(sand.primary.as_deref(), Some("DP-2"));
    assert_eq!(sand.sharp, Some(true));
    assert_eq!(sand.fps, Some(72));
    let scene = policy.scene.unwrap();
    assert_eq!(scene.fps, Some(30));
    assert_eq!(scene.disable_particles, Some(true));
    assert_eq!(scene.assets_dir.as_deref(), Some("/we-assets"));
    assert_eq!(scene.max_dimension, Some(2048));
    assert_eq!(scene.max_effect_chains, Some(4));
    assert_eq!(scene.max_effect_passes, Some(8));
    assert_eq!(scene.strict, Some(false));
}

#[test]
fn policy_fps_clamps() {
    assert_eq!(super::composition::scene_policy_fps(0, false), 1);
    assert_eq!(super::composition::scene_policy_fps(u32::MAX, false), 240);
    assert_eq!(super::composition::scene_policy_fps(u32::MAX, true), 30);
    assert_eq!(super::composition::output_policy_fps(0, 0), 1);
    assert_eq!(super::composition::output_policy_fps(u32::MAX, 2_500_000), 1000);
}

#[test]
fn empty_composition_stops_all() {
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("paper.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(stream.try_clone().unwrap()).read_line(&mut line).unwrap();
        let request: Request = decode_ndjson(&line).unwrap();
        let RequestParams::Stop(stop) = request.params else {
            panic!("expected Paper stop");
        };
        assert!(stop.outputs.is_empty());
        let response = Response {
            id: request.id,
            body: ResponseBody::Success { result: StopResult { stopped: 2 } },
        };
        stream.write_all(encode_ndjson(&response).unwrap().as_bytes()).unwrap();
    });
    let adapter = PaperClientAdapter::new(
        PaperClient::new("unused-paper", &socket),
        Config::from_root(json!({})),
    );
    let config = Config::from_root(json!({}));
    let result =
        adapter.reconcile_composition(&config, &json!({}), &[output("DP-1", 60_000)]).unwrap();
    assert_eq!(result, PaperCompositionResult::Stopped(StopResult { stopped: 2 }));
    server.join().unwrap();
}

#[test]
fn no_outputs_stop_all() {
    let adapter = composition_adapter();
    let config = Config::from_root(json!({}));
    let state = json!({
        "*": crate::audio::entry("static", "/wall/a.png", "", true, 80)
    });
    assert_eq!(
        adapter.composition_plan(&config, &state, &[]).unwrap(),
        PaperCompositionPlan::StopAll
    );
}

#[test]
fn invalid_composition_fails() {
    let adapter = composition_adapter();
    let config = Config::from_root(json!({}));
    let state = json!({
        "DP-1": crate::audio::entry("static", "/wall/a.png", "", true, 80),
        "DP-2": crate::audio::entry("unknown", "/wall/b.png", "", true, 80)
    });
    let error = adapter
        .composition_plan(&config, &state, &[output("DP-1", 60_000), output("DP-2", 60_000)])
        .unwrap_err();
    assert!(error.to_string().contains("unsupported Paper composition source kind unknown"));
}

#[test]
fn rejected_composition_no_retry() {
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("paper.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(stream.try_clone().unwrap()).read_line(&mut line).unwrap();
        let request: Request = decode_ndjson(&line).unwrap();
        let RequestParams::Apply(apply) = request.params else {
            panic!("expected Paper apply");
        };
        assert!(apply.replace_all);
        assert_eq!(apply.assignments.len(), 2);
        assert!(apply.policy.is_some());
        let response = Response::<ApplyResult> {
            id: request.id,
            body: ResponseBody::Failure {
                error: ResponseError { code: "apply_failed".into(), message: "rejected".into() },
            },
        };
        stream.write_all(encode_ndjson(&response).unwrap().as_bytes()).unwrap();
        listener.set_nonblocking(true).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(25));
        assert_eq!(listener.accept().unwrap_err().kind(), std::io::ErrorKind::WouldBlock);
    });
    let adapter = PaperClientAdapter::new(
        PaperClient::new("unused-paper", &socket),
        Config::from_root(json!({})),
    );
    let config = Config::from_root(json!({}));
    let state = json!({
        "DP-1": crate::audio::entry("static", "/wall/a.png", "", true, 80),
        "DP-2": crate::audio::entry("video", "/video/b.mp4", "", false, 55)
    });
    let error = adapter
        .reconcile_composition(&config, &state, &[output("DP-1", 60_000), output("DP-2", 60_000)])
        .unwrap_err();
    assert!(error.to_string().contains("Paper apply_failed: rejected"));
    server.join().unwrap();
}
