use super::*;

fn media(sandbox: &Sandbox, index: usize) -> Vec<(Value, String)> {
    let image = still(sandbox, &format!("image-{index}.png"), "red");
    let video = sandbox.library().join(format!("video-{index}.mp4"));
    assert!(skwd_e2e::ffmpeg_video(&video, "blue", 1.0));
    let scene_id = format!("scene-{index}");
    let video_id = format!("we-video-{index}");
    let scene = sandbox.root.join("we").join(&scene_id);
    let we_video = sandbox.root.join("we").join(&video_id);
    for directory in [&scene, &we_video] {
        std::fs::create_dir_all(directory).unwrap();
    }
    std::fs::write(scene.join("scene.pkg"), b"scene fixture").unwrap();
    std::fs::copy(&image, scene.join("preview.png")).unwrap();
    std::fs::write(scene.join("project.json"), r#"{"type":"scene","preview":"preview.png"}"#)
        .unwrap();
    std::fs::copy(&video, we_video.join("video.mp4")).unwrap();
    std::fs::write(we_video.join("project.json"), r#"{"type":"video","file":"video.mp4"}"#)
        .unwrap();
    vec![
        (json!({"type":"static", "path":image}), image),
        (json!({"type":"video", "path":video}), video.display().to_string()),
        (json!({"type":"we", "we_id":scene_id}), scene.join("preview.png").display().to_string()),
        (json!({"type":"we", "we_id":video_id}), we_video.join("video.mp4").display().to_string()),
    ]
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn plasma_all_media_transitions_preserve_the_outgoing_source_and_policy() {
    let (sandbox, plasma) = plasma_session("plasma-transitions");
    let from = media(&sandbox, 0);
    let to = media(&sandbox, 1);
    let mut config: Value =
        serde_json::from_slice(&std::fs::read(sandbox.config_path()).unwrap()).unwrap();
    config["transition"] = json!({"enabled":true,"shader":"crossfade","durationMs":725,"fps":30});
    config["weRender"] = json!({"fps":12});
    sandbox.write_config(&config);
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let mut failures = Vec::new();
    for (from_request, from_path) in &from {
        for (to_request, _) in &to {
            let mut reset = from_request.clone();
            reset["no_transition"] = json!(true);
            call(&walld, &mut client, "wall.apply", reset);
            for output in ["DP-1", "DP-2"] {
                assert!(assignment(&scripted(&plasma)[output]).transition.is_none());
            }
            call(&walld, &mut client, "wall.apply", to_request.clone());
            let script = scripted(&plasma);
            for output in ["DP-1", "DP-2"] {
                let transition = assignment(&script[output]).transition;
                if !transition.as_ref().is_some_and(|policy| {
                    policy.from.as_deref() == Some(from_path)
                        && policy.effect.as_deref() == Some("crossfade")
                        && policy.duration_ms == Some(725)
                        && policy.fps == Some(30)
                }) {
                    failures.push(format!("{from_request} -> {to_request} on {output}: {transition:?}, expected from {from_path}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}\n{}", failures.join("\n"), walld.log_contents());
    for (to_request, _) in &to {
        for (index, output, volume) in [(0, "DP-1", 25), (1, "DP-2", 75)] {
            let mut request = from[index].0.clone();
            request["output"] = json!(output);
            request["no_transition"] = json!(true);
            call(&walld, &mut client, "wall.apply", request);
            call(
                &walld,
                &mut client,
                "wall.set_audio",
                json!({"outputs":[output],"mute":true,"volume":volume}),
            );
        }
        call(&walld, &mut client, "wall.apply", to_request.clone());
        let script = scripted(&plasma);
        for (index, output, volume) in [(0, "DP-1", 25), (1, "DP-2", 75)] {
            let entry = assignment(&script[output]);
            assert_eq!(entry.transition.unwrap().from.as_deref(), Some(from[index].1.as_str()));
            assert_eq!((entry.mute, entry.volume), (true, volume));
            let expected = if to_request["we_id"] == "scene-1" {
                sandbox.root.join("we/scene-1").display().to_string()
            } else if to_request["we_id"] == "we-video-1" {
                sandbox.root.join("we/we-video-1/video.mp4").display().to_string()
            } else {
                to_request["path"].as_str().unwrap().to_string()
            };
            assert_eq!(entry.source.path, expected);
        }
    }
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn plasma_media_transition_overrides_and_failed_handoffs() {
    let (sandbox, plasma) = plasma_session("plasma-transition-policy");
    let from = media(&sandbox, 0);
    let to = media(&sandbox, 1);
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    for (to_request, _) in &to {
        for (overrides, enabled) in [
            (json!({}), false),
            (json!({"transition":true}), true),
            (json!({"transition":false}), false),
            (json!({"transition":true,"no_transition":true}), false),
        ] {
            call(&walld, &mut client, "wall.apply", from[1].0.clone());
            let mut request = to_request.clone();
            request.as_object_mut().unwrap().extend(overrides.as_object().unwrap().clone());
            request["transition_shader"] = json!("sand-globe");
            request["transition_duration_ms"] = json!(900);
            call(&walld, &mut client, "wall.apply", request);
            for output in ["DP-1", "DP-2"] {
                let policy = assignment(&scripted(&plasma)[output]).transition;
                assert_eq!(policy.is_some(), enabled, "{to_request}: {overrides}");
                if let Some(policy) = policy {
                    assert_eq!(policy.from.as_deref(), Some(from[1].1.as_str()));
                    assert_eq!(policy.effect.as_deref(), Some("sand-globe"));
                    assert_eq!(policy.duration_ms, Some(900));
                }
            }
        }
        call(&walld, &mut client, "wall.apply", from[1].0.clone());
        let previous = sandbox.outputs_json();
        plasma.fail_presentations("transition receiver rejected source");
        let response = client.call("wall.apply", to_request.clone(), 1).unwrap();
        assert!(err_message(Some(&response)).contains("transition receiver rejected source"));
        assert_eq!(sandbox.outputs_json(), previous);
        plasma.confirm_presentations();
    }
}

fn prepared_drawing(sandbox: &Sandbox, source: &str) {
    let layout =
        skwd_wall_core::config::Config::from_root(json!({"paths": {"cache": sandbox.cache()}}));
    let source = std::path::Path::new(source).canonicalize().unwrap();
    let digest = std::process::Command::new("sha256sum").arg(&source).output().unwrap();
    let sha256 = String::from_utf8(digest.stdout).unwrap()[..64].to_string();
    let metadata = std::fs::metadata(&source).unwrap();
    let modified = metadata.modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap();
    let directory = skwd_wall_core::infrastructure::drawing::artifacts(&layout).join(&sha256);
    std::fs::create_dir_all(&directory).unwrap();
    for name in ["drawing.png", "strokes.bin"] {
        std::fs::write(directory.join(name), b"fixture").unwrap();
    }
    let manifest = json!({
        "version": paper_control::DRAWING_VERSION,
        "source": {"path": source, "size": metadata.len(), "mtime_ns": modified.as_nanos(), "sha256": sha256},
        "width": 320, "height": 180, "texture": "drawing.png", "strokes": "strokes.bin",
        "timing_width": 320, "timing_height": 180, "focus": [0.5, 0.5]
    });
    std::fs::write(
        directory.join(skwd_wall_core::infrastructure::drawing::MANIFEST),
        manifest.to_string(),
    )
    .unwrap();
    skwd_wall_core::infrastructure::drawing::publish(
        &layout,
        &source,
        &skwd_wall_core::infrastructure::drawing::CacheEntry {
            artifact: sha256,
            pack: String::new(),
            recipe: skwd_wall_core::infrastructure::drawing::RECIPE.into(),
        },
    )
    .unwrap();
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn plasma_drawing_clears() {
    let (sandbox, plasma) = plasma_session("plasma-drawing");
    let old = still(&sandbox, "old.png", "red");
    let new = still(&sandbox, "new.png", "blue");
    prepared_drawing(&sandbox, &new);
    let mut config: Value =
        serde_json::from_slice(&std::fs::read(sandbox.config_path()).unwrap()).unwrap();
    config["drawing"] = json!({"enabled": true, "durationMs": 4000, "ink": "rainbow"});
    config["transition"] = json!({"enabled": true});
    sandbox.write_config(&config);
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    assert!(wait_until(
        || client
            .call("wall.list", json!({}), 2)
            .and_then(|response| response["result"]["wallpapers"].as_array().cloned())
            .is_some_and(|rows| rows.iter().any(|row| row["key"] == "static:new.png")),
        Duration::from_secs(15)
    ));
    call(
        &walld,
        &mut client,
        "wall.apply",
        json!({"type":"static", "path":old, "no_transition":true}),
    );
    let before = plasma.scripts().len();
    call(&walld, &mut client, "wall.apply", json!({"type":"static", "path":new}));
    let scripts = plasma.scripts().split_off(before);
    assert_eq!(scripts.len(), 2, "{}", walld.log_contents());
    for output in ["DP-1", "DP-2"] {
        let played = assignment(&scripts[0][output]).transition.unwrap();
        let drawing = played.drawing.unwrap();
        assert_eq!(drawing.ink, paper_control::DrawingInk::Rainbow);
        assert_eq!(played.from.as_deref(), Some(old.as_str()));
        assert_eq!(played.duration_ms, Some(4000));
        let settled = assignment(&scripts[1][output]);
        assert!(settled.transition.is_none());
        assert_eq!(settled.source.path, new);
        assert_ne!(scripts[0][output]["presentationId"], scripts[1][output]["presentationId"]);
    }
}
