use super::*;
use serde_json::json;

#[test]
fn preserves_locked_video() {
    let state = WallState::test_new(
        json!({"display":{"outputLocks":{"DP-2":true}},"transition":{"enabled":false}}),
    );
    let outputs = ["DP-1", "DP-2"]
        .map(|name| crate::outputs::OutputInfo { name: name.into(), ..Default::default() });
    let previous = json!({"DP-1":crate::audio::entry("static","/wall/old.png","",true,50),"DP-2":crate::audio::entry("video","/wall/video.mp4","",false,75)});
    let (mut apply, candidate, selected) = plan(
        &state,
        &outputs,
        &previous,
        &std::collections::HashMap::new(),
        &DrawRequest {
            source: Path::new("/wall/new.png"),
            output: ALL_OUTPUTS,
            targets: &[],
            previous_static: None,
            artifact: "/cache/drawing.json".into(),
            media: None,
        },
    )
    .unwrap();
    let video = apply.assignments.iter().find(|assignment| assignment.outputs == ["DP-2"]).unwrap();
    assert_eq!(video.source.path, "/wall/video.mp4");
    assert_eq!(video.source.kind, paper_control::SourceKind::Video);
    assert!(video.transition.is_none());
    assert!(!video.mute);
    assert_eq!(video.volume, 75);
    assert_eq!(candidate["DP-2"], previous["DP-2"]);
    let image = apply.assignments.iter().find(|assignment| assignment.outputs == ["DP-1"]).unwrap();
    assert_eq!(image.source.path, "/wall/new.png");
    assert!(image.transition.as_ref().unwrap().drawing.is_some());
    assert_eq!(apply.policy.as_ref().unwrap().transitions_enabled, Some(false));
    let status = paper_control::StatusResult {
        paused: false,
        policy: apply.policy.clone(),
        assignments: apply
            .assignments
            .iter()
            .map(|assignment| {
                paper_control::AssignmentStatus::from_assignment(assignment, 1, Some(17), true)
            })
            .collect(),
        renderers: Vec::new(),
    };
    assert!(scope(&mut apply, &selected, outputs.len(), &status));
    assert!(!apply.replace_all);
    assert!(apply.policy.is_none());
    assert_eq!(apply.assignments.len(), 1);
    assert_eq!(apply.assignments[0].outputs, ["DP-1"]);
    apply.validate().unwrap();
}

#[test]
fn media_keeps_live_source() {
    for (kind, source, we_id) in
        [(wall_proto::kind::VIDEO, "/wall/clip.mp4", ""), (wall_proto::kind::WE, "/we/42", "42")]
    {
        let state = WallState::test_new(json!({"display":{"outputLocks":{"DP-2":true}}}));
        let old = tempfile::tempdir().unwrap();
        let first = old.path().join("first.png");
        let third = old.path().join("third.png");
        std::fs::write(&first, b"first").unwrap();
        std::fs::write(&third, b"third").unwrap();
        let outputs = ["DP-1", "DP-2", "DP-3"]
            .map(|name| crate::outputs::OutputInfo { name: name.into(), ..Default::default() });
        let staged = crate::audio::entry(
            kind,
            if kind == wall_proto::kind::WE { "" } else { source },
            we_id,
            false,
            62,
        );
        let previous = json!({
            "DP-1":staged.clone(),
            "DP-2":crate::audio::entry("video","/wall/locked.mp4","",false,75),
            "DP-3":staged
        });
        let prior_static = std::collections::HashMap::from([
            (String::from("DP-1"), first.display().to_string()),
            (String::from("DP-3"), third.display().to_string()),
        ]);
        let (apply, candidate, selected) = plan(
            &state,
            &outputs,
            &previous,
            &prior_static,
            &DrawRequest {
                source: Path::new(source),
                output: ALL_OUTPUTS,
                targets: &[],
                previous_static: Some(&prior_static),
                artifact: "/cache/drawing.json".into(),
                media: Some(Media {
                    kind,
                    we_id,
                    mute: false,
                    volume: 62,
                    reference: "/cache/reference.png".into(),
                }),
            },
        )
        .unwrap();
        assert_eq!(selected, ["DP-1", "DP-3"]);
        let status = paper_control::StatusResult {
            paused: false,
            policy: None,
            assignments: Vec::new(),
            renderers: Vec::new(),
        };
        assert!(capture_sources_ready(
            &state,
            &prior_static,
            &selected,
            &apply,
            &status,
            &crate::we::handoff_directory(&state)
        ));
        assert_eq!(candidate["DP-2"], previous["DP-2"]);
        let expected = if we_id.is_empty() {
            source.to_string()
        } else {
            state.config().we_dir().join(we_id).display().to_string()
        };
        assert!(apply.assignments.iter().all(|assignment| assignment.outputs != [ALL_OUTPUTS]));
        for (output, from) in [("DP-1", first.as_path()), ("DP-3", third.as_path())] {
            assert_eq!(candidate[output]["type"], kind);
            let live =
                apply.assignments.iter().find(|assignment| assignment.outputs == [output]).unwrap();
            assert_eq!(live.source.path, expected);
            let transition = live.transition.as_ref().unwrap();
            assert_eq!(
                transition.drawing_source.as_deref(),
                Some(Path::new("/cache/reference.png"))
            );
            assert_eq!(transition.from.as_deref(), from.to_str());
            assert_eq!(live.volume, 62);
        }
        let locked =
            apply.assignments.iter().find(|assignment| assignment.outputs == ["DP-2"]).unwrap();
        assert!(locked.transition.is_none());
    }
}

#[test]
fn we_video_keeps_identity() {
    let root = tempfile::tempdir().unwrap();
    let item = root.path().join("42");
    std::fs::create_dir(&item).unwrap();
    let video = item.join("clip.mp4");
    std::fs::write(&video, b"video").unwrap();
    let state = WallState::test_new(json!({"paths":{"steamWorkshop":root.path()}}));
    let identity = media_identity(
        &state,
        crate::backend::wallpaper::ApplyOutputRequest {
            output: "DP-1",
            kind: wall_proto::kind::VIDEO,
            path: video.to_str().unwrap(),
            we_id: "42",
            fill_mode: "fill",
            mute: true,
            volume: 50,
            transition: None,
            fanout: None,
        },
    );
    assert_eq!(identity.key, "we:42");
    assert_eq!(identity.source, item);
    assert_eq!(identity.live, video);
}

#[test]
fn workshop_video_identity() {
    let root = tempfile::tempdir().unwrap();
    let item = root.path().join("42");
    std::fs::create_dir(&item).unwrap();
    let video = item.join("clip.mp4");
    std::fs::write(&video, b"video").unwrap();
    let state = WallState::test_new(json!({"paths":{"steamWorkshop":root.path()}}));
    let identity = media_identity(
        &state,
        crate::backend::wallpaper::ApplyOutputRequest {
            output: "DP-1",
            kind: wall_proto::kind::VIDEO,
            path: video.to_str().unwrap(),
            we_id: "",
            fill_mode: "fill",
            mute: true,
            volume: 50,
            transition: None,
            fanout: None,
        },
    );
    assert_eq!(identity.key, "we:42");
    assert_eq!(identity.source, item.canonicalize().unwrap());
    assert_eq!(identity.live, video);
}

#[test]
fn locked_output_skips() {
    let config = crate::config::Config::from_root(json!({"display":{"outputLocks":{"DP-2":true}}}));
    assert!(locked_output(&config, "DP-2"));
    assert!(!locked_output(&config, "DP-1"));
    assert!(!locked_output(&config, ALL_OUTPUTS));
}

#[test]
fn external_video_falls_back() {
    let state = WallState::test_new(json!({"drawing":{"enabled":true}}));
    let request = crate::backend::wallpaper::ApplyOutputRequest {
        output: "DP-1",
        kind: wall_proto::kind::VIDEO,
        path: "/outside/rain.mp4",
        we_id: "",
        fill_mode: "fill",
        mute: true,
        volume: 50,
        transition: None,
        fanout: None,
    };
    assert!(!apply_media(&state, request).unwrap());
}

#[test]
fn pinned_source_lives() {
    let root = tempfile::tempdir().unwrap();
    let state = WallState::test_new(json!({"paths":{"cache":root.path()}}));
    let source = root.path().join("next.png");
    std::fs::write(&source, b"next").unwrap();
    let outputs = [crate::outputs::OutputInfo { name: "DP-1".into(), ..Default::default() }];
    let (mut apply, _, _) = plan(
        &state,
        &outputs,
        &json!({}),
        &std::collections::HashMap::new(),
        &DrawRequest {
            source: &source,
            output: "DP-1",
            targets: &[],
            previous_static: None,
            artifact: root.path().join("drawing.json"),
            media: None,
        },
    )
    .unwrap();
    let directory = crate::we::handoff_directory(&state);
    std::fs::create_dir_all(&directory).unwrap();
    let captured = directory.join("frame.ppm");
    std::fs::write(&captured, b"actual A frame").unwrap();
    apply.assignments[0].transition.as_mut().unwrap().from = Some(captured.display().to_string());
    let pinned = pin_transition_sources(&state, &mut apply).unwrap().unwrap();
    let from =
        PathBuf::from(apply.assignments[0].transition.as_ref().unwrap().from.as_ref().unwrap());
    std::fs::remove_file(&captured).unwrap();
    assert_eq!(std::fs::read(&from).unwrap(), b"actual A frame");
    drop(pinned);
    assert!(!from.exists());
}

#[test]
fn keeps_locked_wildcard() {
    let state = WallState::test_new(json!({"display":{"outputLocks":{"DP-2":true}}}));
    let outputs = ["DP-1", "DP-2"]
        .map(|name| crate::outputs::OutputInfo { name: name.into(), ..Default::default() });
    let (mut apply, _, selected) = plan(
        &state,
        &outputs,
        &json!({}),
        &std::collections::HashMap::new(),
        &DrawRequest {
            source: Path::new("/wall/new.png"),
            output: ALL_OUTPUTS,
            targets: &[],
            previous_static: None,
            artifact: "/cache/drawing.json".into(),
            media: None,
        },
    )
    .unwrap();
    let status = paper_control::StatusResult {
        paused: false,
        policy: apply.policy.clone(),
        assignments: vec![paper_control::AssignmentStatus::from_assignment(
            &paper_control::Assignment {
                outputs: vec![ALL_OUTPUTS.into()],
                ..apply.assignments[0].clone()
            },
            1,
            Some(17),
            true,
        )],
        renderers: Vec::new(),
    };
    assert!(!scope(&mut apply, &selected, outputs.len(), &status));
}

#[test]
fn keeps_legacy_video() {
    let state = WallState::test_new(json!({"display":{"outputLocks":{"DP-2":true}}}));
    let outputs = ["DP-1", "DP-2"]
        .map(|name| crate::outputs::OutputInfo { name: name.into(), ..Default::default() });
    let previous = json!({"DP-1":crate::audio::entry("static","/wall/old.png","",true,50),"DP-2":crate::audio::entry("video","/wall/video.mp4","",false,75)});
    let (mut apply, _, selected) = plan(
        &state,
        &outputs,
        &previous,
        &std::collections::HashMap::new(),
        &DrawRequest {
            source: Path::new("/wall/new.png"),
            output: ALL_OUTPUTS,
            targets: &[],
            previous_static: None,
            artifact: "/cache/drawing.json".into(),
            media: None,
        },
    )
    .unwrap();
    let status = paper_control::StatusResult {
        paused: false,
        policy: None,
        assignments: Vec::new(),
        renderers: Vec::new(),
    };
    assert!(scope(&mut apply, &selected, outputs.len(), &status));
    assert!(!apply.replace_all);
    assert_eq!(apply.policy.as_ref().unwrap().transitions_enabled, Some(true));
    assert_eq!(apply.assignments.len(), 1);
    assert_eq!(apply.assignments[0].outputs, ["DP-1"]);
    apply.validate().unwrap();
}

#[test]
fn missing_video_falls_back() {
    let directory = tempfile::tempdir().unwrap();
    let state = WallState::test_new(json!({"paths":{"cache":directory.path()}}));
    let outputs = [crate::outputs::OutputInfo { name: "DP-1".into(), ..Default::default() }];
    let old_static = directory.path().join("old.png");
    std::fs::write(&old_static, b"old A").unwrap();
    let prior_static =
        std::collections::HashMap::from([(String::from("DP-1"), old_static.display().to_string())]);
    let previous = json!({"DP-1":crate::audio::entry("video","/wall/staged-b.mp4","",true,50)});
    let (mut apply, _, selected) = plan(
        &state,
        &outputs,
        &previous,
        &prior_static,
        &DrawRequest {
            source: Path::new("/wall/new.png"),
            output: "DP-1",
            targets: &[],
            previous_static: Some(&prior_static),
            artifact: "/cache/drawing.json".into(),
            media: None,
        },
    )
    .unwrap();
    let mut status = paper_control::StatusResult {
        paused: false,
        policy: None,
        assignments: Vec::new(),
        renderers: Vec::new(),
    };
    let handoffs = crate::we::handoff_directory(&state);
    let actual =
        std::collections::HashMap::from([(String::from("DP-1"), String::from("/wall/old.mp4"))]);
    assert!(!capture_sources_ready(&state, &actual, &selected, &apply, &status, &handoffs));
    assert_eq!(
        apply.assignments[0].transition.as_ref().unwrap().from.as_deref(),
        old_static.to_str()
    );
    assert!(capture_sources_ready(&state, &prior_static, &selected, &apply, &status, &handoffs));
    let scene = directory.path().join("scene");
    std::fs::create_dir(&scene).unwrap();
    let scene_actual =
        std::collections::HashMap::from([(String::from("DP-1"), scene.display().to_string())]);
    assert!(!capture_sources_ready(&state, &scene_actual, &selected, &apply, &status, &handoffs));
    let child = std::process::Command::new("sleep").arg("30").spawn().unwrap();
    state.renderers().set_video_paper("DP-1,DP-3", child, None);
    state.renderers().mark_scene_paper("DP-1,DP-3", true);
    let bare_scene = std::collections::HashMap::from([("DP-1".into(), "1069166728".into())]);
    assert!(snapshot_required(&state, &bare_scene, "DP-1"));
    assert!(!capture_sources_ready(&state, &bare_scene, &selected, &apply, &status, &handoffs));
    assert!(snapshot_required(&state, &prior_static, "DP-1"));

    std::fs::create_dir_all(&handoffs).unwrap();
    let captured = handoffs.join("frame.ppm");
    std::fs::write(&captured, b"live A").unwrap();
    apply.assignments[0].transition.as_mut().unwrap().from = Some(captured.display().to_string());
    assert!(capture_sources_ready(&state, &actual, &selected, &apply, &status, &handoffs));
    assert!(capture_sources_ready(&state, &bare_scene, &selected, &apply, &status, &handoffs));

    std::fs::remove_file(&captured).unwrap();
    status.assignments.push(paper_control::AssignmentStatus::from_assignment(
        &apply.assignments[0],
        1,
        Some(17),
        true,
    ));
    assert!(!capture_sources_ready(&state, &bare_scene, &selected, &apply, &status, &handoffs));
    std::fs::write(&captured, b"actual scene A").unwrap();
    assert!(capture_sources_ready(&state, &bare_scene, &selected, &apply, &status, &handoffs));
    std::fs::remove_file(&captured).unwrap();
    state.renderers().kill_video_paper("DP-1,DP-3");
    status.assignments[0].source =
        paper_control::Source::wallpaper_engine(scene.display().to_string());
    assert!(capture_sources_ready(&state, &scene_actual, &selected, &apply, &status, &handoffs));
    let child = std::process::Command::new("sleep").arg("30").spawn().unwrap();
    state.renderers().set_output_still("DP-1", child, None);
    assert!(snapshot_required(&state, &prior_static, "DP-1"));
    assert!(!capture_sources_ready(&state, &prior_static, &selected, &apply, &status, &handoffs));
    std::fs::write(&captured, b"visible static A").unwrap();
    assert!(capture_sources_ready(&state, &prior_static, &selected, &apply, &status, &handoffs));
    state.renderers().kill_output_still("DP-1");
}

#[test]
fn uses_committed_sources() {
    let directory = tempfile::tempdir().unwrap();
    let old_first = directory.path().join("old-first.png");
    let old_locked = directory.path().join("old-locked.png");
    let old_last = directory.path().join("old-last.png");
    let next = directory.path().join("next.png");
    for path in [&old_first, &old_locked, &old_last, &next] {
        std::fs::write(path, b"image").unwrap();
    }
    let state = WallState::test_new(json!({"display":{"outputLocks":{"DP-2":true}}}));
    let outputs = ["DP-1", "DP-2", "DP-3"]
        .map(|name| crate::outputs::OutputInfo { name: name.into(), ..Default::default() });
    let staged = json!({
        "DP-1": crate::audio::entry("static", next.to_str().unwrap(), "", true, 50),
        "DP-2": crate::audio::entry("static", old_locked.to_str().unwrap(), "", true, 50),
        "DP-3": crate::audio::entry("static", next.to_str().unwrap(), "", true, 50),
    });
    let actual = std::collections::HashMap::from([
        (String::from("DP-1"), old_first.display().to_string()),
        (String::from("DP-2"), old_locked.display().to_string()),
        (String::from("DP-3"), old_last.display().to_string()),
    ]);
    let targets = [String::from("DP-1"), String::from("DP-3")];
    let (apply, candidate, _) = plan(
        &state,
        &outputs,
        &staged,
        &actual,
        &DrawRequest {
            source: &next,
            output: "DP-1",
            targets: &targets,
            previous_static: None,
            artifact: directory.path().join("drawing.json"),
            media: None,
        },
    )
    .unwrap();
    for (output, old) in [("DP-1", &old_first), ("DP-3", &old_last)] {
        let assignment = apply.assignments.iter().find(|item| item.outputs == [output]).unwrap();
        assert_eq!(assignment.source.path, next.display().to_string());
        assert_eq!(assignment.transition.as_ref().unwrap().from.as_deref(), old.to_str());
    }
    let locked = apply.assignments.iter().find(|item| item.outputs == ["DP-2"]).unwrap();
    assert_eq!(locked.source.path, old_locked.display().to_string());
    assert!(locked.transition.is_none());
    assert_eq!(candidate["DP-2"], staged["DP-2"]);

    let prior = std::collections::HashMap::from([
        (String::from("DP-1"), old_first.display().to_string()),
        (String::from("DP-2"), old_locked.display().to_string()),
        (String::from("DP-3"), old_last.display().to_string()),
    ]);
    let mut missing = actual;
    missing.remove("DP-3");
    let (apply, _, _) = plan(
        &state,
        &outputs,
        &staged,
        &missing,
        &DrawRequest {
            source: &next,
            output: "DP-1",
            targets: &targets,
            previous_static: Some(&prior),
            artifact: directory.path().join("drawing.json"),
            media: None,
        },
    )
    .unwrap();
    let last = apply.assignments.iter().find(|item| item.outputs == ["DP-3"]).unwrap();
    assert_eq!(last.transition.as_ref().unwrap().from.as_deref(), old_last.to_str());

    let all = [String::from(ALL_OUTPUTS)];
    let (apply, _, _) = plan(
        &state,
        &outputs,
        &staged,
        &missing,
        &DrawRequest {
            source: &next,
            output: ALL_OUTPUTS,
            targets: &all,
            previous_static: Some(&prior),
            artifact: directory.path().join("drawing.json"),
            media: None,
        },
    )
    .unwrap();
    assert_eq!(apply.assignments.iter().filter(|item| item.transition.is_some()).count(), 2);
}

#[test]
fn rejects_locked_preview() {
    let state = WallState::test_new(json!({"display":{"outputLocks":{"DP-1":true}}}));
    let outputs = [crate::outputs::OutputInfo { name: "DP-1".into(), ..Default::default() }];
    assert!(
        plan(
            &state,
            &outputs,
            &json!({}),
            &std::collections::HashMap::new(),
            &DrawRequest {
                source: Path::new("/wall/a.png"),
                output: "DP-1",
                targets: &[],
                previous_static: None,
                artifact: "/cache/drawing.json".into(),
                media: None,
            }
        )
        .is_err()
    );
}

#[test]
fn bounds_drawing_duration() {
    let state = WallState::test_new(json!({"drawing":{"durationMs":900000}}));
    let outputs = [crate::outputs::OutputInfo { name: "DP-1".into(), ..Default::default() }];
    let (apply, _, _) = plan(
        &state,
        &outputs,
        &json!({}),
        &std::collections::HashMap::new(),
        &DrawRequest {
            source: Path::new("/wall/a.png"),
            output: "DP-1",
            targets: &[],
            previous_static: None,
            artifact: "/cache/drawing.json".into(),
            media: None,
        },
    )
    .unwrap();
    let transition = apply.assignments[0].transition.as_ref().unwrap();
    assert_eq!(transition.duration_ms, Some(20000));
    assert_eq!(transition.from.as_deref(), Some("/wall/a.png"));
    apply.validate().unwrap();
}

#[test]
fn sends_drawing_colours() {
    let chalk = DrawingColours { paper: [20, 24, 22], ink: [250, 250, 245] };
    for (drawing, expected) in [
        (json!({}), DrawingColours::default()),
        (json!({"paperColor":"#141816","inkColor":"fafaf5"}), chalk),
        (json!({"inkColor":"chalk"}), DrawingColours::default()),
    ] {
        let state = WallState::test_new(json!({ "drawing": drawing }));
        let outputs = [crate::outputs::OutputInfo { name: "DP-1".into(), ..Default::default() }];
        let (apply, _, _) = plan(
            &state,
            &outputs,
            &json!({}),
            &std::collections::HashMap::new(),
            &DrawRequest {
                source: Path::new("/wall/a.png"),
                output: "DP-1",
                targets: &[],
                previous_static: None,
                artifact: "/cache/drawing.json".into(),
                media: None,
            },
        )
        .unwrap();
        let colours =
            apply.assignments[0].transition.as_ref().unwrap().drawing.as_ref().unwrap().colours;
        assert_eq!(colours, expected);
    }
}

#[test]
fn sends_drawing_ink() {
    for (drawing, expected) in [
        (json!({}), DrawingInk::Plain),
        (json!({"ink":"rainbow"}), DrawingInk::Rainbow),
        (json!({"ink":"plain"}), DrawingInk::Plain),
        (json!({"ink":"gold"}), DrawingInk::Plain),
    ] {
        let state = WallState::test_new(json!({ "drawing": drawing }));
        let outputs = [crate::outputs::OutputInfo { name: "DP-1".into(), ..Default::default() }];
        let (apply, _, _) = plan(
            &state,
            &outputs,
            &json!({}),
            &std::collections::HashMap::new(),
            &DrawRequest {
                source: Path::new("/wall/a.png"),
                output: "DP-1",
                targets: &[],
                previous_static: None,
                artifact: "/cache/drawing.json".into(),
                media: None,
            },
        )
        .unwrap();
        let ink = apply.assignments[0].transition.as_ref().unwrap().drawing.as_ref().unwrap().ink;
        assert_eq!(ink, expected);
    }
}

#[test]
fn card_apply_choice() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first.png");
    let second = directory.path().join("nested/second.png");
    std::fs::create_dir_all(second.parent().unwrap()).unwrap();
    std::fs::write(&first, b"first").unwrap();
    std::fs::write(&second, b"second").unwrap();
    let root = json!({"paths":{"wallpaper":directory.path()}, "drawing":{
        "enabled":false, "wallpapers":{"static:first.png":true}
    }});
    let state = WallState::test_new(root.clone());
    assert_eq!(state.library_key(&first).as_deref(), Some("static:first.png"));
    assert_eq!(state.library_key(&second).as_deref(), Some("static:nested/second.png"));
    assert!(enabled(&state, &root, &first));
    assert!(!enabled(&state, &root, &second));

    let global = json!({"paths":{"wallpaper":directory.path()}, "drawing":{
        "enabled":true, "wallpapers":{"static:nested/second.png":false}
    }});
    assert!(enabled(&state, &global, &first));
    assert!(!enabled(&state, &global, &second));
}

#[test]
fn stationary_video_plan() {
    if crate::infrastructure::wallpaper::application::stationary_child(
        "infrastructure::wallpaper::application::drawing::tests::stationary_video_plan",
    ) {
        return;
    }
    let state = WallState::test_new(json!({
        "paper": {"videoEngine": "tinier"},
        "niri": {"overviewBackdrop": true, "overviewMode": "stationary"}
    }));
    let outputs = [crate::outputs::OutputInfo { name: "DP-1".into(), ..Default::default() }];
    let (apply, _, _) = plan(
        &state,
        &outputs,
        &json!({"DP-1": crate::audio::entry("video", "/wall/video.mp4", "", true, 50)}),
        &std::collections::HashMap::new(),
        &DrawRequest {
            source: Path::new("/wall/video.mp4"),
            output: "DP-1",
            targets: &[],
            previous_static: None,
            artifact: "/cache/drawing.json".into(),
            media: Some(Media {
                kind: wall_proto::kind::VIDEO,
                we_id: "",
                mute: true,
                volume: 50,
                reference: "/cache/reference.json".into(),
            }),
        },
    )
    .unwrap();
    assert!(apply.policy.as_ref().unwrap().surface.is_some());
    assert_eq!(apply.assignments[0].source, paper_control::Source::video("/wall/video.mp4"));
}

#[test]
fn plasma_plan_per_output() {
    let directory = tempfile::tempdir().unwrap();
    let old = directory.path().join("old.png");
    std::fs::write(&old, b"old").unwrap();
    let old = old.to_str().unwrap();
    let state = WallState::test_new(
        json!({"display":{"outputLocks":{"DP-3":true}},"drawing":{"ink":"rainbow"}}),
    );
    let outputs = ["DP-1", "DP-2", "DP-3"]
        .map(|name| crate::outputs::OutputInfo { name: name.into(), ..Default::default() });
    let previous = json!({"*":crate::audio::entry("static",old,"",true,50)});
    let actual = std::collections::HashMap::from([("DP-1".to_string(), old.to_string())]);
    let PlasmaPlan { candidate, transitions, selected } = plasma_plan(
        &state,
        &outputs,
        &previous,
        &actual,
        &DrawRequest {
            source: Path::new("/wall/clip.mp4"),
            output: ALL_OUTPUTS,
            targets: &[],
            previous_static: None,
            artifact: "/cache/drawing.json".into(),
            media: Some(Media {
                kind: wall_proto::kind::VIDEO,
                we_id: "",
                mute: false,
                volume: 40,
                reference: "/cache/reference.png".into(),
            }),
        },
    )
    .unwrap();
    assert_eq!(selected, ["DP-1", "DP-2"]);
    assert_eq!(transitions.keys().collect::<Vec<_>>(), ["DP-1", "DP-2"]);
    assert_eq!(candidate["DP-3"], previous["*"]);
    assert_eq!(candidate["DP-1"]["path"], "/wall/clip.mp4");
    for transition in transitions.values() {
        assert_eq!(transition.from.as_deref(), Some(old));
        assert_eq!(transition.drawing_source.as_deref(), Some(Path::new("/cache/reference.png")));
        assert_eq!(transition.drawing.as_ref().unwrap().ink, DrawingInk::Rainbow);
        assert_eq!(transition.fps, None);
        transition.validate().unwrap();
    }
}
