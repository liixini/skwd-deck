use super::*;
use paper_control::{Assignment, AssignmentStatus, FillMode, Layer, Source};
use serde_json::json;
use std::path::{Path, PathBuf};

fn source_video(root: &Path) -> PathBuf {
    let original = root.join("source.mp4");
    std::fs::write(&original, b"original-video-bytes").unwrap();
    original
}

fn status() -> StatusResult {
    StatusResult {
        paused: false,
        policy: None,
        assignments: vec![AssignmentStatus::from_assignment(
            &Assignment {
                outputs: vec!["DP-1".into()],
                source: Source::static_file("/wall/old.png"),
                fill_mode: FillMode::default(),
                background: paper_control::Background::default(),
                mute: true,
                volume: 50,
                layer: Layer::Background,
                transition: None,
                system_audio: None,
            },
            1,
            Some(123),
            true,
        )],
        renderers: vec![],
    }
}

fn request() -> ApplyOutputRequest<'static> {
    ApplyOutputRequest {
        output: "DP-1",
        kind: wall_proto::kind::STATIC,
        path: "/wall/new.png",
        we_id: "",
        fill_mode: "fill",
        mute: true,
        volume: 50,
        transition: Some(crate::backend::wallpaper::OutputTransitionRequest {
            enabled: true,
            shader: "circle",
            duration_ms: 1500,
        }),
        fanout: None,
    }
}

#[test]
fn sharing_cohort_rejoins() {
    for (shared, locked, independent, count) in [
        (true, false, false, 2),
        (false, false, false, 1),
        (true, true, false, 1),
        (true, false, true, 1),
    ] {
        let state = WallState::test_new(json!({
            "display": {"outputLocks": {"DP-2": locked}}
        }));
        state.renderers().set_independent_playback(independent);
        let outputs = ["DP-1", "DP-2"]
            .map(|name| crate::outputs::OutputInfo { name: name.into(), ..Default::default() });
        let previous = json!({
            "DP-1": crate::audio::entry("static", "/wall/old.png", "", true, 50),
            "DP-2": crate::audio::entry("static", "/wall/new.png", "", true, 50)
        });
        let mut status = status();
        status.policy = Some(paper_control::RendererPolicy {
            shared_outputs: Some(shared),
            ..Default::default()
        });
        let mut second = status.assignments[0].clone();
        second.outputs = vec!["DP-2".into()];
        second.source = Source::static_file("/wall/new.png");
        second.pid = Some(124);
        status.assignments.push(second);
        let (apply, candidate, selected) =
            plan(&state, &outputs, &previous, &status, request(), &[]).unwrap();
        assert_eq!(selected, ["DP-1"]);
        assert_eq!(apply.assignments.len(), count);
        assert!(apply.assignments.iter().all(|assignment| assignment.transition.is_some()));
        assert_eq!(candidate["DP-2"], previous["DP-2"]);
    }
}

#[test]
fn retains_other_output() {
    let state = WallState::test_new(json!({}));
    let outputs = ["DP-1", "DP-2"]
        .map(|name| crate::outputs::OutputInfo { name: name.into(), ..Default::default() });
    let previous = json!({
        "DP-1": crate::audio::entry("static", "/wall/old.png", "", true, 50),
        "DP-2": crate::audio::entry("video", "/wall/movie.mp4", "", false, 75)
    });
    let (apply, candidate, selected) =
        plan(&state, &outputs, &previous, &status(), request(), &[]).unwrap();
    assert_eq!(selected, ["DP-1"]);
    assert!(!apply.replace_all);
    assert!(apply.policy.is_none());
    assert_eq!(apply.assignments.len(), 1);
    assert_eq!(candidate["DP-2"], previous["DP-2"]);
    let transition = apply.assignments[0].transition.as_ref().unwrap();
    assert_eq!(transition.effect.as_deref(), Some("circle"));
    assert_eq!(transition.duration_ms, Some(1500));
    assert!(transition.drawing.is_none());
    assert!(transition.from.is_none());
}

#[test]
fn selects_fanout_targets() {
    let state = WallState::test_new(json!({"display":{"outputLocks":{"DP-2":true}}}));
    let outputs = ["DP-1", "DP-2", "DP-3"]
        .map(|name| crate::outputs::OutputInfo { name: name.into(), ..Default::default() });
    let previous = json!({
        "DP-1": crate::audio::entry("static", "/wall/new.png", "", true, 50),
        "DP-2": crate::audio::entry("static", "/wall/locked.png", "", true, 50),
        "DP-3": crate::audio::entry("static", "/wall/new.png", "", true, 50),
    });
    let mut status = status();
    let mut last = status.assignments[0].clone();
    last.outputs = vec!["DP-3".into()];
    status.assignments.push(last);
    let targets = [String::from("DP-1"), String::from("DP-3")];
    let request = request();
    assert!(can_handoff(&state.config(), &outputs, &status, request, &targets));
    let (apply, candidate, selected) =
        plan(&state, &outputs, &previous, &status, request, &targets).unwrap();
    assert_eq!(selected, targets);
    assert_eq!(apply.assignments.len(), 2);
    assert!(apply.assignments.iter().all(|item| item.transition.is_some()));
    assert_eq!(candidate["DP-2"], previous["DP-2"]);

    let all = [String::from(ALL_OUTPUTS)];
    assert!(can_handoff(&state.config(), &outputs, &status, request, &all));
    let (_, _, selected) = plan(&state, &outputs, &previous, &status, request, &all).unwrap();
    assert_eq!(selected, targets);
}

#[test]
fn retains_locked_output() {
    let state = WallState::test_new(json!({"display":{"outputLocks":{"DP-2":true}}}));
    let outputs = ["DP-1", "DP-2"]
        .map(|name| crate::outputs::OutputInfo { name: name.into(), ..Default::default() });
    let previous = json!({"*":crate::audio::entry("static", "/wall/old.png", "", true, 50)});
    let mut request = request();
    request.output = ALL_OUTPUTS;
    request.transition = Some(crate::backend::wallpaper::OutputTransitionRequest {
        enabled: false,
        shader: "circle",
        duration_ms: 1500,
    });
    let (apply, candidate, selected) =
        plan(&state, &outputs, &previous, &status(), request, &[]).unwrap();
    assert_eq!(selected, ["DP-1"]);
    assert_eq!(candidate["DP-2"][PATH], "/wall/old.png");
    assert!(apply.assignments[0].transition.is_none());
}

#[test]
fn expands_incumbent_wildcard() {
    let state = WallState::test_new(json!({}));
    let outputs = ["DP-1", "DP-2"]
        .map(|name| crate::outputs::OutputInfo { name: name.into(), ..Default::default() });
    let previous = json!({"*":crate::audio::entry("static", "/wall/old.png", "", true, 50)});
    let mut status = status();
    status.assignments[0].outputs = vec![ALL_OUTPUTS.into()];
    let (apply, _, _) = plan(&state, &outputs, &previous, &status, request(), &[]).unwrap();
    assert!(apply.replace_all);
    assert_eq!(apply.assignments.len(), 2);
    let retained =
        apply.assignments.iter().find(|assignment| assignment.outputs == ["DP-2"]).unwrap();
    assert_eq!(retained.source.path, "/wall/old.png");
    assert!(retained.transition.is_none());
}

#[test]
fn rejects_mixed_ownership() {
    let state = WallState::test_new(json!({}));
    let outputs = ["DP-1", "DP-2"]
        .map(|name| crate::outputs::OutputInfo { name: name.into(), ..Default::default() });
    let config = state.config();
    let mut request = request();
    assert!(can_handoff(&config, &outputs, &status(), request, &[]));
    request.output = "DP-2";
    assert!(!can_handoff(&config, &outputs, &status(), request, &[]));
    request.output = ALL_OUTPUTS;
    assert!(!can_handoff(&config, &outputs, &status(), request, &[]));
}

#[test]
fn respects_disabled_policy() {
    let state = WallState::test_new(json!({}));
    let outputs = [crate::outputs::OutputInfo { name: "DP-1".into(), ..Default::default() }];
    let mut status = status();
    status.policy = Some(paper_control::RendererPolicy {
        transitions_enabled: Some(false),
        ..Default::default()
    });
    let mut request = request();
    assert!(!can_handoff(&state.config(), &outputs, &status, request, &[]));
    request.transition.as_mut().unwrap().enabled = false;
    assert!(can_handoff(&state.config(), &outputs, &status, request, &[]));
}

#[test]
fn wildcard_video_transition() {
    let temp = tempfile::tempdir().unwrap();
    let state = WallState::test_new(json!({}));
    let original = source_video(temp.path());
    let outputs = ["DP-1", "DP-2"]
        .map(|name| crate::outputs::OutputInfo { name: name.into(), ..Default::default() });
    let previous = json!({"*": crate::audio::entry("static", "/wall/old.png", "", true, 50)});
    let mut status = status();
    status.assignments[0].outputs = vec![ALL_OUTPUTS.into()];
    let mut request = request();
    request.output = ALL_OUTPUTS;
    request.kind = wall_proto::kind::VIDEO;
    request.path = original.to_str().unwrap();
    request.mute = false;
    let (apply, candidate, selected) =
        plan(&state, &outputs, &previous, &status, request, &[]).unwrap();
    assert_eq!(selected, ["DP-1", "DP-2"]);
    assert!(apply.replace_all);
    assert_eq!(apply.assignments.len(), 2);
    assert_eq!(apply.assignments.iter().filter(|item| !item.mute).count(), 1);
    for assignment in &apply.assignments {
        assert_eq!(assignment.source.path, original.to_str().unwrap());
        assert_eq!(assignment.source.kind, paper_control::SourceKind::Video);
        assert!(assignment.transition.is_some());
        assert_eq!(candidate[&assignment.outputs[0]][PATH], original.to_str().unwrap());
    }
}

#[test]
fn retained_default_audio() {
    let temp = tempfile::tempdir().unwrap();
    let state = WallState::test_new(json!({}));
    let original = source_video(temp.path());
    for (retained, selected) in [("DP-1", "DP-2"), ("DP-3", "DP-1")] {
        let outputs = [retained, selected]
            .map(|name| crate::outputs::OutputInfo { name: name.into(), ..Default::default() });
        let mut previous = json!({});
        previous[retained] =
            crate::audio::entry("video", original.to_str().unwrap(), "", false, 75);
        previous[selected] = crate::audio::entry("static", "/wall/old.png", "", true, 50);
        let mut status = status();
        status.assignments[0].outputs = vec![retained.into()];
        status.assignments[0].source = Source::video(original.to_str().unwrap());
        status.assignments[0].mute = false;
        let mut next = status.assignments[0].clone();
        next.outputs = vec![selected.into()];
        next.source = Source::static_file("/wall/old.png");
        next.mute = true;
        status.assignments.push(next);
        let mut request = request();
        request.output = selected;
        request.kind = wall_proto::kind::VIDEO;
        request.path = original.to_str().unwrap();
        request.mute = false;
        let (apply, _, _) = plan(&state, &outputs, &previous, &status, request, &[]).unwrap();
        assert!(!apply.replace_all);
        assert_eq!(apply.assignments.len(), 1);
        assert_eq!(apply.assignments[0].source.path, original.to_str().unwrap());
        assert_eq!(apply.assignments[0].source.kind, paper_control::SourceKind::Video);
        assert!(apply.assignments[0].mute);
        assert!(apply.assignments[0].transition.is_some());
    }
}

#[test]
fn stationary_video_plan() {
    if crate::infrastructure::wallpaper::application::stationary_child(
        "infrastructure::wallpaper::application::handoff::tests::stationary_video_plan",
    ) {
        return;
    }
    let state = WallState::test_new(json!({
        "paper": {"videoEngine": "tinier"},
        "niri": {"overviewBackdrop": true, "overviewMode": "stationary"}
    }));
    let outputs = [crate::outputs::OutputInfo { name: "DP-1".into(), ..Default::default() }];
    let config = state.config();
    assert!(crate::infrastructure::paper::renderer_policy(&config, &outputs).surface.is_some());
    let mut request = request();
    request.kind = wall_proto::kind::VIDEO;
    request.path = "/wall/video.mp4";
    let previous = json!({"DP-1": crate::audio::entry("video", "/wall/old.mp4", "", true, 50)});
    let (apply, candidate, _) = plan(&state, &outputs, &previous, &status(), request, &[]).unwrap();
    assert_eq!(apply.assignments[0].source, Source::video("/wall/video.mp4"));
    assert_eq!(candidate["DP-1"][PATH], "/wall/video.mp4");
}
