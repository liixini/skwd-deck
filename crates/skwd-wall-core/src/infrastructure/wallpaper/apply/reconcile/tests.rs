use super::super::transition::{previous_media_source, transition_primary, transitions_for_output};
use super::super::wallpaper_engine::{PreparedWe, PreparedWeMode};
use super::{ReadyHandoff, RendererLaunchSpec, prepare_batch};
use crate::WallState;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

fn sleeper() -> std::process::Child {
    Command::new("sleep").arg("60").spawn().expect("spawn sleeper")
}

fn executable(path: &Path) {
    std::fs::write(path, b"#!/bin/sh\nexec cat\n").unwrap();
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    std::fs::set_permissions(path, permissions).unwrap();
}

#[test]
fn mixed_batch_late_candidate_exit_preserves_every_incumbent_and_assignment() {
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("renderer");
    executable(&binary);
    let state = WallState::test_new(serde_json::json!({
        "paths": {
            "paperStillBin": binary.display().to_string(),
            "paperVkBin": binary.display().to_string()
        }
    }));

    let static_incumbent = sleeper();
    let static_incumbent_pid = static_incumbent.id();
    state.renderers().restore_output_still("DP-S", (static_incumbent, None));
    let video_incumbent = sleeper();
    let video_incumbent_pid = video_incumbent.id();
    state.renderers().restore_video_paper_state("DP-V", (video_incumbent, None), false);
    state.renderers().set_assignment("DP-S", "/w/old.png");
    state.renderers().set_assignment("DP-V", "/v/old.mp4");
    let assignments_before = state.renderers().assignments();

    let static_startup =
        RendererLaunchSpec::static_for("DP-S", "/w/new.png", "fill").spawn(&state).unwrap();
    let video_startup =
        RendererLaunchSpec::video_for("DP-V", vec!["DP-V".to_string(), "/v/new.mp4".to_string()])
            .spawn(&state)
            .unwrap();
    let static_candidate_pid = static_startup.pid();
    let video_candidate_pid = video_startup.pid();
    state.renderers().signal_ready(static_candidate_pid);
    state.renderers().signal_ready(video_candidate_pid);
    let static_ready = static_startup.wait_ready().unwrap();
    let video_ready = video_startup.wait_ready().unwrap();

    unsafe {
        libc::kill(video_candidate_pid.cast_signed(), libc::SIGKILL);
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while state.renderers().wallpaper_pids().contains(&video_candidate_pid) {
        state.renderers().reap_exited();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }

    let result = prepare_batch(
        vec![
            ReadyHandoff {
                renderer: static_ready,
                assignments: vec![("DP-S".to_string(), "/w/new.png".to_string())],
                transition_duration: None,
            },
            ReadyHandoff {
                renderer: video_ready,
                assignments: vec![("DP-V".to_string(), "/v/new.mp4".to_string())],
                transition_duration: None,
            },
        ],
        PreparedWe {
            groups: std::collections::BTreeMap::new(),
            audio: std::collections::BTreeMap::new(),
            mode: PreparedWeMode::Keep { audio_changed: false },
        },
        Vec::new(),
    );
    let error = result.err().expect("later dead candidate must abort the batch");

    assert!(error.to_string().contains("exited before commit"));
    assert!(state.renderers().output_still_pid_alive("DP-S", static_incumbent_pid));
    assert_eq!(state.renderers().video_paper_pid("DP-V"), Some(video_incumbent_pid));
    assert_eq!(state.renderers().assignments(), assignments_before);
    for pid in [static_incumbent_pid, video_incumbent_pid] {
        assert!(Path::new(&format!("/proc/{pid}")).exists());
    }
    for pid in [static_candidate_pid, video_candidate_pid] {
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
    }
    state.renderers().kill_all();
}

#[test]
fn mixed_batch_late_native_exit_preserves_scene_state_and_static_incumbent() {
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("renderer");
    executable(&binary);
    let workshop = directory.path().join("we");
    let item = workshop.join("4242");
    std::fs::create_dir_all(&item).unwrap();
    std::fs::write(item.join("scene.pkg"), b"fixture").unwrap();
    let state = WallState::test_new(serde_json::json!({
        "paths": {
            "paperStillBin": binary.display().to_string(),
            "paperVkBin": binary.display().to_string(),
            "steamWorkshop": workshop.display().to_string()
        }
    }));

    let static_incumbent = sleeper();
    let static_incumbent_pid = static_incumbent.id();
    state.renderers().restore_output_still("DP-S", (static_incumbent, None));
    let scene_incumbent = sleeper();
    let scene_incumbent_pid = scene_incumbent.id();
    state.renderers().restore_video_paper_state("DP-W", (scene_incumbent, None), true);
    state.renderers().set_assignment("DP-S", "/w/old.png");
    state.renderers().set_assignment("DP-W", "old-scene");
    state.renderers().set_policy("scenepolicy", "old-policy");
    let old_groups =
        std::collections::BTreeMap::from([("old-scene".to_string(), vec!["DP-W".to_string()])]);
    let old_audio = std::collections::BTreeMap::from([("old-scene".to_string(), (true, 100))]);
    state.renderers().set_we_render(old_groups.clone(), old_audio.clone());
    let assignments_before = state.renderers().assignments();

    let static_startup =
        RendererLaunchSpec::static_for("DP-S", "/w/new.png", "fill").spawn(&state).unwrap();
    let static_candidate_pid = static_startup.pid();
    state.renderers().signal_ready(static_candidate_pid);
    let static_ready = static_startup.wait_ready().unwrap();

    let stop = AtomicBool::new(false);
    let native = std::thread::scope(|scope| {
        let readiness = scope.spawn(|| {
            while !stop.load(Ordering::Relaxed) {
                for pid in state.renderers().wallpaper_pids() {
                    state.renderers().signal_ready(pid);
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        });
        let native =
            crate::we::spawn_scene_for(&state, &["DP-W".to_string()], "4242", true, 100, false);
        stop.store(true, Ordering::Relaxed);
        readiness.join().unwrap();
        native
    })
    .unwrap();
    let native_candidate_pid = state.renderers().video_paper_pid("DP-W").unwrap();

    unsafe {
        libc::kill(native_candidate_pid.cast_signed(), libc::SIGKILL);
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while state.renderers().wallpaper_pids().contains(&native_candidate_pid) {
        state.renderers().reap_exited();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }

    let result = prepare_batch(
        vec![ReadyHandoff {
            renderer: static_ready,
            assignments: vec![("DP-S".to_string(), "/w/new.png".to_string())],
            transition_duration: None,
        }],
        PreparedWe {
            groups: std::collections::BTreeMap::from([(
                "4242".to_string(),
                vec!["DP-W".to_string()],
            )]),
            audio: std::collections::BTreeMap::from([("4242".to_string(), (true, 100))]),
            mode: PreparedWeMode::Replace(vec![native]),
        },
        Vec::new(),
    );
    let error = result.err().expect("later dead native candidate must abort the batch");

    assert!(error.to_string().contains("exited before commit"));
    assert!(state.renderers().output_still_pid_alive("DP-S", static_incumbent_pid));
    assert_eq!(state.renderers().video_paper_pid("DP-W"), Some(scene_incumbent_pid));
    assert!(state.renderers().is_scene_paper("DP-W"));
    assert_eq!(state.renderers().assignments(), assignments_before);
    assert_eq!(state.renderers().policy("scenepolicy").as_deref(), Some("old-policy"));
    assert!(state.renderers().we_render_matches(&old_groups, &old_audio));
    for pid in [static_incumbent_pid, scene_incumbent_pid] {
        assert!(Path::new(&format!("/proc/{pid}")).exists());
    }
    for pid in [static_candidate_pid, native_candidate_pid] {
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
    }
    state.renderers().kill_all();
}

#[test]
fn we_preview_media() {
    let directory = tempfile::tempdir().unwrap();
    let workshop = directory.path().join("we");
    let item = workshop.join("123");
    std::fs::create_dir_all(&item).unwrap();
    let preview = item.join("preview.gif");
    std::fs::write(&preview, b"gif").unwrap();
    let image = directory.path().join("image.webp");
    std::fs::write(&image, b"webp").unwrap();
    let state = WallState::test_new(serde_json::json!({
        "paths": {"steamWorkshop": workshop.display().to_string()}
    }));

    assert_eq!(previous_media_source(&state, "DP-1", "123").as_deref(), preview.to_str());
    assert_eq!(
        previous_media_source(&state, "DP-1", item.to_str().unwrap()).as_deref(),
        preview.to_str()
    );
    assert_eq!(
        previous_media_source(&state, "DP-1", image.to_str().unwrap()).as_deref(),
        image.to_str()
    );
    assert_eq!(previous_media_source(&state, "DP-1", ""), None);
    assert_eq!(previous_media_source(&state, "DP-1", "../123"), None);
}

#[test]
fn primary_scope_stable_output() {
    let outputs = vec!["DP-4".to_string(), "DP-5".to_string(), "eDP-1".to_string()];
    let stale = WallState::test_new(serde_json::json!({
        "transition": {"sandScope": "primary", "sandPrimary": "e-DP1"}
    }));
    let selected = WallState::test_new(serde_json::json!({
        "transition": {
            "shaderScopes": {"fade": "primary", "sand-donut": "primary"},
            "sandPrimary": "eDP-1"
        }
    }));

    assert_eq!(transition_primary(&stale, &outputs, "sand-donut").as_deref(), Some("DP-4"));
    assert_eq!(transition_primary(&selected, &outputs, "sand-donut").as_deref(), Some("eDP-1"));
    assert_eq!(transition_primary(&selected, &outputs, "fade").as_deref(), Some("eDP-1"));
    assert!(transitions_for_output(true, Some("DP-4"), "DP-4"));
    assert!(!transitions_for_output(true, Some("DP-4"), "eDP-1"));
}

#[test]
fn shader_scope_defaults() {
    let state = WallState::test_new(serde_json::json!({
        "transition": {
            "sandScope": "primary",
            "sandPrimary": "DP-4",
            "shaderScopes": {"glitch": "primary", "sand-donut": "all"}
        }
    }));
    let outputs = vec!["DP-4".to_string(), "eDP-1".to_string()];

    assert_eq!(transition_primary(&state, &outputs, "fade"), None);
    assert_eq!(transition_primary(&state, &outputs, "sand-donut"), None);
    assert_eq!(transition_primary(&state, &outputs, "glitch").as_deref(), Some("DP-4"));
}
