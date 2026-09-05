#![cfg(test)]

use super::*;

fn sleeper() -> std::process::Child {
    std::process::Command::new("sleep").arg("60").spawn().expect("spawn sleep")
}

#[test]
fn commit_replaces_compatibility() {
    let state = WallState::test_new(serde_json::json!({}));
    let old_compatibility = sleeper();
    let old_pid = old_compatibility.id();
    state.renderers().track(old_compatibility);
    let native = sleeper();
    let native_pid = native.id();
    state.renderers().set_video_paper("DP-1,DP-2", native, None);
    state.renderers().mark_scene_paper("DP-1,DP-2", true);

    commit_scene_set(
        &state,
        vec![NativeSceneCandidate {
            key: "DP-1,DP-2".to_string(),
            renderer: None,
            properties: (String::new(), serde_json::Map::new()),
        }],
    )
    .unwrap();

    assert!(!std::path::Path::new(&format!("/proc/{old_pid}")).exists());
    assert!(state.renderers().fleet_pids().is_empty());
    assert_eq!(state.renderers().video_paper_pid("DP-1,DP-2"), Some(native_pid));
    state.renderers().kill_all();
}

#[test]
fn native_scene_replaces_video() {
    let state = WallState::test_new(serde_json::json!({}));
    let old_video = sleeper();
    let old_pid = old_video.id();
    state.renderers().set_video_paper("*", old_video, None);
    let native = sleeper();
    let native_pid = native.id();
    state.renderers().set_video_paper("DP-1,DP-2", native, None);
    state.renderers().mark_scene_paper("DP-1,DP-2", true);

    commit_scene_set(
        &state,
        vec![NativeSceneCandidate {
            key: "DP-1,DP-2".to_string(),
            renderer: None,
            properties: (String::new(), serde_json::Map::new()),
        }],
    )
    .unwrap();
    for renderer in state.renderers().take_video_papers_except(&["DP-1,DP-2".to_string()]) {
        kill_held_renderer(renderer);
    }

    assert!(!std::path::Path::new(&format!("/proc/{old_pid}")).exists());
    assert!(!state.renderers().has_video_paper("*"));
    assert_eq!(state.renderers().video_paper_pid("DP-1,DP-2"), Some(native_pid));
    state.renderers().kill_all();
}

#[test]
fn we_id_traversal() {
    assert!(valid_we_id("431960"));
    assert!(valid_we_id("3605908472"));
    assert!(!valid_we_id(""));
    assert!(!valid_we_id("../../etc"));
    assert!(!valid_we_id("a/b"));
    assert!(!valid_we_id(".."));
    assert!(!valid_we_id("-config"));
    assert!(!valid_we_id("--assets-dir"));
}

#[test]
fn safe_join_blocks_escape() {
    let directory = tempfile::tempdir().unwrap();
    let base = directory.path().join("item");
    std::fs::create_dir_all(base.join("sub")).unwrap();
    std::fs::write(base.join("clip.mp4"), b"video").unwrap();
    std::fs::write(base.join("sub/clip.mp4"), b"video").unwrap();
    assert_eq!(safe_item_join(&base, "clip.mp4"), Some(base.join("clip.mp4")));
    assert_eq!(safe_item_join(&base, "sub/clip.mp4"), Some(base.join("sub/clip.mp4")));
    assert_eq!(safe_item_join(&base, "../../../etc/passwd"), None);
    assert_eq!(safe_item_join(&base, "/etc/passwd"), None);

    let outside = directory.path().join("outside.mp4");
    std::fs::write(&outside, b"outside").unwrap();
    std::os::unix::fs::symlink(&outside, base.join("escape.mp4")).unwrap();
    assert_eq!(safe_item_join(&base, "escape.mp4"), None);
}

fn capture_state(directory: &tempfile::TempDir) -> WallState {
    let state = WallState::test_new(serde_json::json!({
        "paths": {"cache": directory.path().join("cache").display().to_string()},
    }));
    std::fs::create_dir_all(state.config().cache_dir()).unwrap();
    state
}

#[test]
fn live_scene_capture_returns_an_atomic_frame_and_restores_pause_policy() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("scene.stdin");
    let state = capture_state(&directory);
    let (child, stdin) = crate::infrastructure::renderers::capture_child(&input);
    state.renderers().set_video_paper("DP-1", child, stdin);
    state.renderers().mark_scene_paper("DP-1", true);

    let observed = input.clone();
    let writer = std::thread::spawn(move || {
        for _ in 0..200 {
            if let Ok(text) = std::fs::read_to_string(&observed)
                && let Some(line) = text.lines().next()
            {
                let command: serde_json::Value = serde_json::from_str(line).unwrap();
                let target = command["freeze"].as_str().expect("freeze target");
                std::fs::write(target, b"P6\n2 2\n255\n\xff\0\0\0\xff\0\0\0\xff\xff\xff\xff")
                    .unwrap();
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("freeze request was not written");
    });

    let frame =
        capture_transition_frame_with_timeout(&state, "DP-1", std::time::Duration::from_secs(1))
            .expect("captured scene frame");
    writer.join().unwrap();
    assert!(std::fs::metadata(&frame).is_ok_and(|metadata| metadata.len() > 16));

    let commands = (0..100)
        .find_map(|_| {
            let text = std::fs::read_to_string(&input).unwrap_or_default();
            let lines = text.lines().collect::<Vec<_>>();
            if lines.len() < 2 {
                std::thread::sleep(std::time::Duration::from_millis(2));
                return None;
            }
            Some(
                lines
                    .into_iter()
                    .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
                    .collect::<Vec<_>>(),
            )
        })
        .expect("freeze and pause-policy commands");
    assert_eq!(commands[0]["freeze"], frame);
    assert_eq!(commands[1]["pause"], false);
    std::fs::remove_file(frame).unwrap();
    state.renderers().kill_all();
}

#[test]
fn scene_capture_timeout_and_renderer_death_leave_no_partial_files() {
    let timeout_directory = tempfile::tempdir().unwrap();
    let timeout_input = timeout_directory.path().join("scene.stdin");
    let timeout_state = capture_state(&timeout_directory);
    let (child, stdin) = crate::infrastructure::renderers::capture_child(&timeout_input);
    timeout_state.renderers().set_video_paper("DP-1", child, stdin);
    timeout_state.renderers().mark_scene_paper("DP-1", true);
    assert!(
        capture_transition_frame_with_timeout(
            &timeout_state,
            "DP-1",
            std::time::Duration::from_millis(25),
        )
        .is_none()
    );
    let handoffs =
        std::path::PathBuf::from(timeout_state.config().cache_dir()).join("scene-handoffs");
    assert_eq!(std::fs::read_dir(handoffs).unwrap().count(), 0);
    timeout_state.renderers().kill_all();

    let death_directory = tempfile::tempdir().unwrap();
    let death_state = capture_state(&death_directory);
    let mut child = std::process::Command::new("sh")
        .arg("-c")
        .arg("IFS= read -r line; exit 0")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let stdin = child.stdin.take();
    death_state.renderers().set_video_paper("DP-1", child, stdin);
    death_state.renderers().mark_scene_paper("DP-1", true);
    let started = std::time::Instant::now();
    assert!(
        capture_transition_frame_with_timeout(
            &death_state,
            "DP-1",
            std::time::Duration::from_secs(2),
        )
        .is_none()
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    let handoffs =
        std::path::PathBuf::from(death_state.config().cache_dir()).join("scene-handoffs");
    assert_eq!(std::fs::read_dir(handoffs).unwrap().count(), 0);
    death_state.renderers().kill_all();
}

#[test]
fn project_type_default() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(read_project_type(dir.path()), ("scene".to_string(), String::new()));
    std::fs::write(dir.path().join("project.json"), r#"{"type":"Video","file":"clip.mp4"}"#)
        .unwrap();
    assert_eq!(read_project_type(dir.path()), ("video".to_string(), "clip.mp4".to_string()));
}

#[test]
fn transition_media_uses_incumbent_assignment() {
    let directory = tempfile::tempdir().unwrap();
    let image = directory.path().join("current.webp");
    std::fs::write(&image, b"image").unwrap();
    let state = WallState::test_new(serde_json::json!({}));
    state.renderers().set_assignment("DP-1", &image.display().to_string());

    assert_eq!(previous_media(&state, &["DP-1".to_string()]).as_deref(), image.to_str());
    assert_eq!(previous_media(&state, &["DP-2".to_string()]), None);
}

#[test]
fn apply_we_bails() {
    let dir = tempfile::tempdir().unwrap();
    let we = dir.path().join("we");
    std::fs::create_dir_all(we.join("123")).unwrap();
    std::fs::write(
        we.join("123").join("project.json"),
        r#"{"type": "video", "file": "../../evil.mp4"}"#,
    )
    .unwrap();
    for (id, project) in [
        ("124", r#"{"type": "video"}"#),
        ("125", r#"{"type": "web"}"#),
        ("126", r#"{"type": "application"}"#),
    ] {
        std::fs::create_dir_all(we.join(id)).unwrap();
        std::fs::write(we.join(id).join("project.json"), project).unwrap();
    }
    let ws = we.display().to_string();
    let state = WallState::test_new(serde_json::json!({ "paths": { "steamWorkshop": ws } }));
    let disabled = WallState::test_new(serde_json::json!({ "features": { "steam": false } }));
    let msg = |result: anyhow::Result<Option<String>>| result.unwrap_err().to_string();
    assert!(msg(apply_we(&disabled, "123")).contains("disabled"));
    assert!(msg(apply_we(&state, "../123")).contains("invalid WE id"));
    assert!(msg(apply_we(&state, "999")).contains("not found"));
    assert!(msg(apply_we(&state, "123")).contains("unsafe"));
    assert!(msg(apply_we(&state, "124")).contains("no media file"));
    assert!(msg(apply_we(&state, "125")).contains("unsupported type \"web\""));
    assert!(msg(apply_we(&state, "126")).contains("unsupported type \"application\""));
}

#[test]
fn project_title_trims() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(read_project_title(dir.path()), None);
    let project = dir.path().join("project.json");
    std::fs::write(&project, r#"{"title": "  Neon City  "}"#).unwrap();
    assert_eq!(read_project_title(dir.path()).as_deref(), Some("Neon City"));
    std::fs::write(&project, r#"{"title": "   "}"#).unwrap();
    assert_eq!(read_project_title(dir.path()), None);
    std::fs::write(&project, r#"{"title": 5}"#).unwrap();
    assert_eq!(read_project_title(dir.path()), None);
    std::fs::write(&project, r#"{"title": "x""#).unwrap();
    assert_eq!(read_project_title(dir.path()), None);
}

#[test]
fn project_type_malformed() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project.json");
    std::fs::write(&project, "{broken").unwrap();
    assert_eq!(read_project_type(dir.path()), ("scene".to_string(), String::new()));
    std::fs::write(&project, r#"{"type": 7, "file": null}"#).unwrap();
    assert_eq!(read_project_type(dir.path()), ("scene".to_string(), String::new()));
}

#[test]
fn video_project_detect() {
    assert!(is_video_project("video", "clip.mp4"));
    assert!(is_video_project("Video", "clip.mp4"));
    assert!(!is_video_project("video", ""));
    assert!(!is_video_project("scene", "clip.mp4"));
    assert!(!is_video_project("web", ""));
}

#[test]
fn supported_project_types() {
    assert!(is_supported_project("scene"));
    assert!(is_supported_project("video"));
    assert!(!is_supported_project(""));
    assert!(!is_supported_project("web"));
    assert!(!is_supported_project("Web"));
    assert!(!is_supported_project("WEB"));
    assert!(!is_supported_project("application"));
}

#[test]
fn find_preview_prefix() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("project.json"), "{}").unwrap();
    std::fs::write(dir.path().join("preview.gif"), "x").unwrap();
    assert!(find_preview(dir.path()).unwrap().ends_with("preview.gif"));
}

#[test]
fn scene_key_selected_outputs() {
    assert_eq!(scene_renderer_key(&[]), "*");
    assert_eq!(scene_renderer_key(&["*".into()]), "*");
    assert_eq!(
        scene_renderer_key(&["HDMI-A-1".into(), "DP-1".into(), "DP-1".into()]),
        "DP-1,HDMI-A-1"
    );
}

#[test]
fn scene_process_output_set() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    let dir = tempfile::tempdir().unwrap();
    let item = dir.path().join("we/42");
    std::fs::create_dir_all(&item).unwrap();
    std::fs::write(item.join("scene.pkg"), b"probe bypassed").unwrap();
    let bin = dir.path().join("renderer");
    let args_path = dir.path().join("args");
    std::fs::write(
        &bin,
        format!(
            "#!/bin/sh\n: > '{}'\nfor a in \"$@\"; do printf '%s\\n' \"$a\" >> '{}'; done\nexec cat\n",
            args_path.display(),
            args_path.display()
        ),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&bin).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    std::fs::set_permissions(&bin, permissions).unwrap();
    let state = WallState::test_new(serde_json::json!({
        "paths": {
            "steamWorkshop": dir.path().join("we").display().to_string(),
            "paperVkBin": bin.display().to_string(),
            "cache": dir.path().join("cache").display().to_string(),
        },
        "weRender": {"native": true},
    }));
    std::fs::create_dir_all(state.config().cache_dir()).unwrap();

    let stop = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let ready = scope.spawn(|| {
            while !stop.load(Ordering::Relaxed) {
                for pid in state.renderers().wallpaper_pids() {
                    state.renderers().signal_ready(pid);
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        });
        let candidate =
            spawn_scene_for(&state, &["DP-2".into(), "DP-1".into()], "42", true, 100, true)
                .unwrap();
        commit_scene_set(&state, vec![candidate]).unwrap();
        stop.store(true, Ordering::Relaxed);
        ready.join().unwrap();
    });

    let args = std::fs::read_to_string(args_path).unwrap();
    let args: Vec<&str> = args.lines().collect();
    assert_eq!(args.first().copied(), Some("DP-1,DP-2"));
    assert!(args.windows(2).any(|pair| pair == ["--mute", "true"]));
    assert!(args.windows(2).any(|pair| pair == ["--volume", "100"]));
    assert!(state.renderers().has_video_paper("DP-1,DP-2"));
    assert!(state.renderers().is_scene_paper("DP-1,DP-2"));
    state.renderers().kill_all();
}

fn warm_scene_command(transitions: bool, no_transition: bool) -> serde_json::Value {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    let dir = tempfile::tempdir().unwrap();
    for id in ["42", "43"] {
        let item = dir.path().join("we").join(id);
        std::fs::create_dir_all(&item).unwrap();
        std::fs::write(item.join("scene.pkg"), b"probe bypassed").unwrap();
    }
    let bin = dir.path().join("renderer");
    let input = dir.path().join("input");
    std::fs::write(
        &bin,
        format!(
            "#!/bin/sh\nwhile IFS= read -r line; do printf '%s\\n' \"$line\" >> '{}'; done\n",
            input.display()
        ),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&bin).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    std::fs::set_permissions(&bin, permissions).unwrap();
    let state = WallState::test_new(serde_json::json!({
        "paths": {
            "steamWorkshop": dir.path().join("we").display().to_string(),
            "paperVkBin": bin.display().to_string(),
            "cache": dir.path().join("cache").display().to_string(),
        },
        "transition": {"enabled": transitions, "durationMs": 725, "shader": "sand-globe"},
        "weRender": {"native": true},
    }));
    state.apply().set_no_transition(no_transition);
    std::fs::create_dir_all(state.config().cache_dir()).unwrap();

    let stop = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let ready = scope.spawn(|| {
            while !stop.load(Ordering::Relaxed) {
                for pid in state.renderers().wallpaper_pids() {
                    state.renderers().signal_ready(pid);
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        });
        let first = spawn_scene_for(&state, &["DP-1".into()], "42", true, 100, true).unwrap();
        commit_scene_set(&state, vec![first]).unwrap();
        let second = spawn_scene_for(&state, &["DP-1".into()], "43", true, 100, true).unwrap();
        assert!(second.renderer.is_none());
        commit_scene_set(&state, vec![second]).unwrap();
        stop.store(true, Ordering::Relaxed);
        ready.join().unwrap();
    });

    let line = (0..100)
        .find_map(|_| {
            let line = std::fs::read_to_string(&input)
                .ok()
                .and_then(|text| text.lines().next().map(str::to_string));
            if line.is_none() {
                std::thread::sleep(Duration::from_millis(2));
            }
            line
        })
        .expect("warm command captured");
    state.renderers().kill_all();
    serde_json::from_str(&line).unwrap()
}

#[test]
fn warm_swap_transition_policy() {
    let disabled = warm_scene_command(false, false);
    assert!(disabled["to"].as_str().unwrap().ends_with("/we/43"));
    assert_eq!(disabled["mute"], true);
    assert_eq!(disabled["volume"], 100);
    assert!(disabled.get("shader").is_none());
    assert!(disabled.get("duration_ms").is_none());

    let suppressed = warm_scene_command(true, true);
    assert!(suppressed.get("shader").is_none());
    assert!(suppressed.get("duration_ms").is_none());

    let enabled = warm_scene_command(true, false);
    assert!(enabled["to"].as_str().unwrap().ends_with("/we/43"));
    assert_eq!(enabled["mute"], true);
    assert_eq!(enabled["volume"], 100);
    assert_eq!(enabled["shader"], "sand-globe");
    assert_eq!(enabled["duration_ms"], 725);
}

#[test]
fn preview_metadata_takes_precedence_and_stays_inside_item() {
    let dir = tempfile::tempdir().unwrap();
    let item = dir.path().join("item");
    std::fs::create_dir_all(item.join("images")).unwrap();
    std::fs::write(item.join("preview.png"), b"fallback").unwrap();
    std::fs::write(item.join("images/thumbnail.png"), b"preferred").unwrap();
    std::fs::write(item.join("project.json"), r#"{"preview":"images/thumbnail.png"}"#).unwrap();
    assert_eq!(find_preview(&item), Some(item.join("images/thumbnail.png")));
    std::fs::write(dir.path().join("outside.png"), b"outside").unwrap();
    std::os::unix::fs::symlink(dir.path().join("outside.png"), item.join("escape.png")).unwrap();
    for preview in ["../outside.png", "/etc/passwd", "escape.png", "images", "missing.png"] {
        std::fs::write(
            item.join("project.json"),
            serde_json::json!({"preview":preview}).to_string(),
        )
        .unwrap();
        assert_eq!(find_preview(&item), Some(item.join("preview.png")));
    }
}
