#![cfg(test)]

use super::process_map::{ChildMap, kill_all, kill_one, reap, replace, retain};
use super::readiness::ReadinessRegistry;
use super::supervisor::{RendererSupervisor, capture_child, exited_child, kill_held_renderer};
use crate::lock;
use std::collections::HashMap;
use std::process::Child;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn dummy() -> Child {
    Command::new("sleep").arg("60").spawn().expect("spawn sleep for test")
}

fn reaped(pid: u32) -> bool {
    !std::path::Path::new(&format!("/proc/{pid}")).exists()
}

fn process_state(pid: u32) -> Option<char> {
    std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()?
        .rsplit_once(')')?
        .1
        .split_whitespace()
        .next()?
        .chars()
        .next()
}

fn wait_process_state(pid: u32, want: impl Fn(char) -> bool) -> Option<char> {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        let state = process_state(pid)?;
        if want(state) || std::time::Instant::now() >= deadline {
            return Some(state);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn retain_reaps_rest() {
    let mut map: ChildMap = HashMap::new();
    let mut pids = HashMap::new();
    for output in ["DP-1", "DP-2", "DP-3"] {
        let child = dummy();
        pids.insert(output, child.id());
        map.insert(output.to_string(), (child, None));
    }
    retain(&mut map, &["DP-2".to_string()]);
    assert_eq!(map.len(), 1);
    assert!(map.contains_key("DP-2"));
    assert!(reaped(pids["DP-1"]));
    assert!(reaped(pids["DP-3"]));
    assert!(!reaped(pids["DP-2"]));
    let kept = pids["DP-2"];
    kill_all(&mut map);
    assert!(map.is_empty());
    assert!(reaped(kept));
}

#[test]
fn replace_reaps_old() {
    let mut map: ChildMap = HashMap::new();
    let first = dummy();
    let first_pid = first.id();
    map.insert("DP-1".to_string(), (first, None));
    replace(&mut map, "DP-1", dummy(), None);
    assert_eq!(map.len(), 1);
    assert!(reaped(first_pid));
    let second_pid = map["DP-1"].0.id();
    assert!(!reaped(second_pid));
    kill_one(&mut map, "DP-1");
    assert!(map.is_empty());
    assert!(reaped(second_pid));
}

#[test]
fn reap_removes_exited() {
    let mut map: ChildMap = HashMap::new();
    let (exited, _) = exited_child();
    let live = dummy();
    let live_pid = live.id();
    map.insert("dead".to_string(), (exited, None));
    map.insert("live".to_string(), (live, None));
    for _ in 0..50 {
        if reap(&mut map) == ["dead"] {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(map.keys().cloned().collect::<Vec<_>>(), ["live"]);
    kill_all(&mut map);
    assert!(reaped(live_pid));
}

#[test]
fn lock_survives_poison() {
    let mutex = Arc::new(Mutex::new(1u32));
    let poisoner = mutex.clone();
    let _ = std::thread::spawn(move || {
        let _guard = poisoner.lock().unwrap();
        panic!("poison");
    })
    .join();
    assert!(mutex.is_poisoned());
    *lock(&mutex) = 2;
    assert_eq!(*lock(&mutex), 2);
}

#[test]
fn ready_signal_first() {
    let registry = ReadinessRegistry::default();
    registry.signal(42);
    assert!(registry.wait(42, Duration::from_millis(50)));
}

#[test]
fn ready_wait_wakes() {
    let registry = Arc::new(ReadinessRegistry::default());
    let waiting = Arc::clone(&registry);
    let waiter = std::thread::spawn(move || waiting.wait(7, Duration::from_secs(5)));
    std::thread::sleep(Duration::from_millis(60));
    registry.signal(7);
    assert!(waiter.join().unwrap());
}

#[test]
fn ready_wait_times_out() {
    assert!(!ReadinessRegistry::default().wait(99, Duration::from_millis(30)));
}

#[test]
fn ready_gates_prune() {
    let registry = ReadinessRegistry::default();
    for pid in 0..5u32 {
        registry.signal(pid);
    }
    assert_eq!(registry.len(), 5);
    registry.expire_all();
    registry.signal(99);
    assert_eq!(registry.len(), 1);
}

#[test]
fn ready_wait_consumes() {
    let registry = ReadinessRegistry::default();
    registry.signal(5);
    assert!(registry.wait(5, Duration::from_millis(50)));
    assert!(!registry.wait(5, Duration::from_millis(30)));
}

fn state() -> RendererSupervisor {
    RendererSupervisor::default()
}

#[test]
fn departed_assignments_dropped() {
    let st = state();
    st.set_assignment("DP-4", "/v/a.mp4");
    st.set_assignment("DP-5", "/v/a.mp4");
    st.set_assignment("multi", "DP-4=/v/a.mp4;DP-5=/v/a.mp4");
    assert!(st.retain_live_output_assignments(&[String::from("DP-4")]));
    let assignments = st.assignments();
    assert!(!assignments.contains_key("DP-4"));
    assert!(!assignments.contains_key("DP-5"));
    assert!(!assignments.contains_key("multi"));
}

#[test]
fn departed_output_renderers_are_killed() {
    let st = state();
    let (live, live_stdin) = (dummy(), None);
    let (gone, gone_stdin) = (dummy(), None);
    let live_pid = live.id();
    let gone_pid = gone.id();
    st.set_video_paper("DP-1", live, live_stdin);
    st.set_video_paper("DP-3", gone, gone_stdin);

    let departed = st.kill_departed_outputs(&[String::from("DP-1")]);

    assert_eq!(departed, vec![String::from("DP-3")]);
    assert!(reaped(gone_pid), "DP-3 renderer still running after its output left");
    assert!(!reaped(live_pid), "DP-1 renderer must survive its output staying connected");
    assert!(st.has_video_paper("DP-1"));
    assert!(!st.has_video_paper("DP-3"));
}

#[test]
fn an_unknown_output_set_reaps_nothing() {
    let st = state();
    let (still, still_stdin) = (dummy(), None);
    let (video, video_stdin) = (dummy(), None);
    let still_pid = still.id();
    let video_pid = video.id();
    st.set_output_still("DP-1", still, still_stdin);
    st.set_video_paper("DP-2", video, video_stdin);

    assert!(st.kill_departed_outputs(&[]).is_empty());

    assert!(!reaped(still_pid), "a still renderer was killed on an unknown output set");
    assert!(!reaped(video_pid), "a video renderer was killed on an unknown output set");
    assert!(st.has_video_paper("DP-2"));
}

#[test]
fn wildcard_renderers_survive_a_departure() {
    let st = state();
    let (wildcard, stdin) = (dummy(), None);
    let pid = wildcard.id();
    st.set_video_paper("*", wildcard, stdin);

    // "*" and "multi" are not outputs; a departure must not reap them.
    assert!(st.kill_departed_outputs(&[String::from("DP-1")]).is_empty());
    assert!(!reaped(pid));
    assert!(st.has_video_paper("*"));
}

#[test]
fn unrelated_departure_keeps_multi() {
    let st = state();
    st.set_assignment("DP-1", "/v/a.mp4");
    st.set_assignment("DP-2", "/v/a.mp4");
    st.set_assignment("DP-3", "/w/a.png");
    st.set_assignment("multi", "DP-1=/v/a.mp4;DP-2=/v/a.mp4");
    assert!(!st.retain_live_output_assignments(&[String::from("DP-1"), String::from("DP-2")]));
    let assignments = st.assignments();
    assert_eq!(assignments.get("DP-1").map(String::as_str), Some("/v/a.mp4"));
    assert_eq!(assignments.get("DP-2").map(String::as_str), Some("/v/a.mp4"));
    assert!(!assignments.contains_key("DP-3"));
    assert_eq!(assignments.get("multi").map(String::as_str), Some("DP-1=/v/a.mp4;DP-2=/v/a.mp4"));
}

fn json_lines(path: &std::path::Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).expect("renderer stdin lines must be JSON"))
        .collect()
}

#[test]
fn still_swap_line() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("still.stdin");
    let st = state();
    let (child, stdin) = capture_child(&out);
    st.set_base_still(child, stdin);
    assert!(st.still_swap("/w/new.png", "fit"));
    let (mut child, stdin) = st.take_base_still().expect("base still stays registered after swap");
    drop(stdin);
    let _ = child.wait();
    assert_eq!(
        std::fs::read_to_string(&out).unwrap(),
        "{\"path\":\"/w/new.png\",\"fill\":\"fit\"}\n",
    );
}

#[test]
fn still_swap_no_base() {
    assert!(!state().still_swap("/w/x.png", "fill"));
}

#[test]
fn delayed_retirement_targets_pid() {
    let supervisor = Arc::new(state());
    let first = dummy();
    let first_pid = first.id();
    supervisor.restore_paper((first, None));
    supervisor.retire_paper_after(first_pid, Duration::from_millis(20));
    std::thread::sleep(Duration::from_millis(80));
    assert!(reaped(first_pid));

    let replacement = dummy();
    let replacement_pid = replacement.id();
    supervisor.restore_paper((replacement, None));
    supervisor.retire_paper_after(first_pid, Duration::from_millis(20));
    std::thread::sleep(Duration::from_millis(80));
    assert_eq!(supervisor.paper_pid(), Some(replacement_pid));
    assert!(!reaped(replacement_pid));
    supervisor.kill_paper();
}

#[test]
fn still_swap_carries_fill() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("still.stdin");
    let st = state();
    let (child, stdin) = capture_child(&out);
    st.set_base_still(child, stdin);
    assert!(st.still_swap("/w/new.png", "center"));
    let (mut child, stdin) = st.take_base_still().expect("base still stays registered");
    drop(stdin);
    let _ = child.wait();
    assert_eq!(
        std::fs::read_to_string(&out).unwrap(),
        "{\"path\":\"/w/new.png\",\"fill\":\"center\"}\n",
    );
}

fn swap_eventually_fails(mut swap: impl FnMut() -> bool) -> bool {
    for _ in 0..50 {
        if !swap() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    false
}

#[test]
fn still_swap_dead_clears() {
    let st = state();
    let (child, stdin) = exited_child();
    st.set_base_still(child, stdin);
    assert!(st.has_base_still());
    assert!(swap_eventually_fails(|| st.still_swap("/w/x.png", "fill")),);
    assert!(!st.has_base_still());
}

#[test]
fn video_swap_line() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("dp1.stdin");
    let st = state();
    let (child, stdin) = capture_child(&out);
    st.set_video_paper("DP-1", child, stdin);
    assert!(!st.video_swap("DP-2", "/v/b.mp4", false, 50));
    assert!(st.video_swap("DP-1", "/v/b.mp4", false, 250));
    for (mut child, stdin) in st.take_all_video_papers() {
        drop(stdin);
        let _ = child.wait();
    }
    assert_eq!(
        json_lines(&out),
        vec![serde_json::json!({"to": "/v/b.mp4", "mute": false, "volume": 100})]
    );
}

#[test]
fn grouped_scene_freeze_targets_served_output() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("scene.stdin");
    let st = state();
    let (child, stdin) = capture_child(&out);
    st.set_video_paper("DP-1,DP-2", child, stdin);
    st.mark_scene_paper("DP-1,DP-2", true);

    let handle =
        st.freeze_scene("DP-2", "/cache/live-scene.ppm").expect("grouped scene serves DP-2");
    assert_eq!(handle.key, "DP-1,DP-2");
    st.finish_scene_freeze(&handle);
    assert!(st.freeze_scene("DP-3", "/cache/wrong.ppm").is_none());

    for (mut child, stdin) in st.take_all_video_papers() {
        drop(stdin);
        let _ = child.wait();
    }
    assert_eq!(
        json_lines(&out),
        vec![
            serde_json::json!({"to": "", "freeze": "/cache/live-scene.ppm"}),
            serde_json::json!({"to": "", "pause": false}),
        ]
    );
}

#[test]
fn scene_freeze_liveness_tracks_exit_and_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let st = state();
    let original_input = directory.path().join("original.stdin");
    let (live, stdin) = capture_child(&original_input);
    st.set_video_paper("DP-1", live, stdin);
    st.mark_scene_paper("DP-1", true);
    let handle = st.freeze_scene("DP-1", "/cache/live.ppm").expect("freeze handle");
    assert!(st.scene_freeze_alive(&handle));

    let replacement_input = directory.path().join("replacement.stdin");
    let (replacement, stdin) = capture_child(&replacement_input);
    st.set_video_paper("DP-1", replacement, stdin);
    assert!(!st.scene_freeze_alive(&handle));
    st.finish_scene_freeze(&handle);
    std::thread::sleep(Duration::from_millis(10));
    assert!(std::fs::read_to_string(replacement_input).unwrap_or_default().is_empty());
    for renderer in st.take_all_video_papers() {
        kill_held_renderer(renderer);
    }
}

#[test]
fn paper_swap_line() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("paper.stdin");
    let st = state();
    assert!(!st.paper_swap("/v/n.mp4", "fade", 600, true, 80));
    let (child, stdin) = capture_child(&out);
    st.set_paper_stdin(stdin);
    assert!(st.swap_paper(child).is_none());
    assert!(st.paper_swap("/v/n.mp4", "fade", 600, true, 250));
    drop(st.take_paper_stdin());
    let (mut child, stdin) = st.take_paper().unwrap();
    drop(stdin);
    let _ = child.wait();
    assert_eq!(
        json_lines(&out),
        vec![serde_json::json!({
            "to": "/v/n.mp4", "shader": "fade", "duration_ms": 600,
            "mute": true, "volume": 100
        })],
    );
}

#[test]
fn paper_swap_dead_clears() {
    let st = state();
    let (child, stdin) = exited_child();
    st.set_paper_stdin(stdin);
    st.swap_paper(child);
    assert!(swap_eventually_fails(|| st.paper_swap("/v/n.mp4", "fade", 600, true, 80)),);
    drop(st.take_paper());
    assert!(st.take_paper_stdin().is_none());
}

#[test]
fn send_audio_filter() {
    let dir = tempfile::tempdir().unwrap();
    let files: Vec<std::path::PathBuf> =
        ["dp1", "dp2", "star", "paper"].iter().map(|name| dir.path().join(name)).collect();
    let st = state();
    for (name, file) in [("DP-1", &files[0]), ("DP-2", &files[1]), ("*", &files[2])] {
        let (child, stdin) = capture_child(file);
        st.set_video_paper(name, child, stdin);
    }
    let (child, stdin) = capture_child(&files[3]);
    st.set_paper_stdin(stdin);
    st.swap_paper(child);
    st.send_audio(Some(&["DP-1".to_string()]), Some(true), Some(250));
    st.send_audio(None, None, None);
    for (mut child, stdin) in st.take_all_video_papers() {
        drop(stdin);
        let _ = child.wait();
    }
    drop(st.take_paper_stdin());
    let (mut child, stdin) = st.take_paper().unwrap();
    drop(stdin);
    let _ = child.wait();
    let full = serde_json::json!({"to": "", "mute": true, "volume": 100});
    let bare = serde_json::json!({"to": ""});
    assert_eq!(json_lines(&files[0]), vec![full.clone(), bare.clone()]);
    assert_eq!(json_lines(&files[1]), vec![bare.clone()]);
    assert_eq!(json_lines(&files[2]), vec![full.clone(), bare.clone()]);
    assert_eq!(json_lines(&files[3]), vec![full, bare]);
}

#[test]
fn send_audio_grouped_scene() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("scene.stdin");
    let st = state();
    let (child, stdin) = capture_child(&file);
    st.set_video_paper("DP-2,DP-3", child, stdin);

    st.send_audio(Some(&["DP-2".to_string()]), Some(true), Some(0));

    for (mut child, stdin) in st.take_all_video_papers() {
        drop(stdin);
        let _ = child.wait();
    }
    assert_eq!(json_lines(&file), vec![serde_json::json!({"to": "", "mute": true, "volume": 0})]);
}

#[test]
fn shared_audio_only_wildcard() {
    let dir = tempfile::tempdir().unwrap();
    let files: Vec<std::path::PathBuf> =
        ["dp1", "star", "paper"].iter().map(|name| dir.path().join(name)).collect();
    let st = state();
    for (name, file) in [("DP-1", &files[0]), ("*", &files[1])] {
        let (child, stdin) = capture_child(file);
        st.set_video_paper(name, child, stdin);
    }
    let (child, stdin) = capture_child(&files[2]);
    st.set_paper_stdin(stdin);
    st.swap_paper(child);

    st.send_shared_video_audio(false, 21);

    for (mut child, stdin) in st.take_all_video_papers() {
        drop(stdin);
        let _ = child.wait();
    }
    drop(st.take_paper_stdin());
    let (mut child, stdin) = st.take_paper().unwrap();
    drop(stdin);
    let _ = child.wait();

    let expected = serde_json::json!({"to": "", "mute": false, "volume": 21});
    assert!(json_lines(&files[0]).is_empty());
    assert_eq!(json_lines(&files[1]), vec![expected]);
    assert!(json_lines(&files[2]).is_empty());
}

#[test]
fn multi_audio_routed_by_output() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("multi");
    let st = state();
    let (child, stdin) = capture_child(&file);
    st.set_video_paper("multi", child, stdin);

    st.send_audio(Some(&["DP-2".to_string()]), Some(false), Some(36));
    st.send_multi_video_audio(&["DP-1".to_string(), "DP-2".to_string()], true, 250);

    for (mut child, stdin) in st.take_all_video_papers() {
        drop(stdin);
        let _ = child.wait();
    }
    assert_eq!(
        json_lines(&file),
        vec![
            serde_json::json!({
                "to": "", "mute": false, "volume": 36, "outputs": ["DP-2"]
            }),
            serde_json::json!({
                "to": "", "mute": true, "volume": 100,
                "outputs": ["DP-1", "DP-2"]
            }),
        ]
    );
}

#[test]
fn render_pause_broadcast() {
    let dir = tempfile::tempdir().unwrap();
    let paper_out = dir.path().join("paper.stdin");
    let vid_out = dir.path().join("dp1.stdin");
    let st = state();
    let (child, stdin) = capture_child(&paper_out);
    st.set_paper_stdin(stdin);
    st.swap_paper(child);
    let (child, stdin) = capture_child(&vid_out);
    st.set_video_paper("DP-1", child, stdin);
    st.set_paused(true);
    st.set_paused(false);
    for (mut child, stdin) in st.take_all_video_papers() {
        drop(stdin);
        let _ = child.wait();
    }
    drop(st.take_paper_stdin());
    let (mut child, stdin) = st.take_paper().unwrap();
    drop(stdin);
    let _ = child.wait();
    let want = "{\"to\":\"\",\"pause\":true}\n{\"to\":\"\",\"pause\":false}\n";
    assert_eq!(std::fs::read_to_string(&paper_out).unwrap(), want);
    assert_eq!(std::fs::read_to_string(&vid_out).unwrap(), want);
}

#[test]
fn pause_sessions_compose() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("paper.stdin");
    let st = state();
    let (child, stdin) = capture_child(&output);
    st.set_paper_stdin(stdin);
    st.swap_paper(child);
    st.set_session_paused(11, true);
    st.set_paused(true);
    st.set_session_paused(22, true);
    st.set_session_paused(11, false);
    st.set_paused(false);
    st.set_session_paused(22, false);
    drop(st.take_paper_stdin());
    let (mut child, stdin) = st.take_paper().unwrap();
    drop(stdin);
    let _ = child.wait();
    let want = "{\"to\":\"\",\"pause\":true}\n{\"to\":\"\",\"pause\":false}\n";
    assert_eq!(std::fs::read_to_string(&output).unwrap(), want);
}

#[test]
fn new_renderer_inherits_pause() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("paper.stdin");
    let st = state();
    st.set_session_paused(11, true);
    let (child, stdin) = capture_child(&output);
    st.set_video_paper("DP-1", child, stdin);
    st.set_session_paused(11, false);
    for (mut child, stdin) in st.take_all_video_papers() {
        drop(stdin);
        let _ = child.wait();
    }
    let want = "{\"to\":\"\",\"pause\":true}\n{\"to\":\"\",\"pause\":false}\n";
    assert_eq!(std::fs::read_to_string(&output).unwrap(), want);
}

#[test]
fn apply_window_releases_pause() {
    let dir = tempfile::tempdir().unwrap();
    let old_output = dir.path().join("old.stdin");
    let new_output = dir.path().join("new.stdin");
    let st = state();
    let (old_child, old_stdin) = capture_child(&old_output);
    st.set_video_paper("DP-1", old_child, old_stdin);
    st.set_session_paused(11, true);
    st.begin_apply();
    st.begin_apply();
    let (new_child, new_stdin) = capture_child(&new_output);
    st.set_video_paper("DP-2", new_child, new_stdin);
    st.end_apply();
    st.end_apply();
    st.set_session_paused(11, false);
    for (mut child, stdin) in st.take_all_video_papers() {
        drop(stdin);
        let _ = child.wait();
    }
    assert_eq!(
        std::fs::read_to_string(&old_output).unwrap(),
        "{\"to\":\"\",\"pause\":true}\n{\"to\":\"\",\"pause\":false}\n{\"to\":\"\",\"pause\":true}\n{\"to\":\"\",\"pause\":false}\n"
    );
    assert_eq!(
        std::fs::read_to_string(&new_output).unwrap(),
        "{\"to\":\"\",\"pause\":true}\n{\"to\":\"\",\"pause\":false}\n"
    );
}

#[test]
fn session_waits_for_transition() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("transition.stdin");
    let st = Arc::new(state());
    st.set_session_paused(11, true);
    st.begin_apply();
    let (child, stdin) = capture_child(&output);
    let pid = child.id();
    st.restore_paper((child, stdin));
    st.allow_session_rendering_for(pid, Duration::from_millis(30));
    st.end_apply();
    std::thread::sleep(Duration::from_millis(10));
    assert_eq!(std::fs::read_to_string(&output).unwrap(), "");
    std::thread::sleep(Duration::from_millis(80));
    st.set_session_paused(11, false);
    let (mut child, stdin) = st.take_paper().unwrap();
    drop(stdin);
    let _ = child.wait();
    assert_eq!(
        std::fs::read_to_string(&output).unwrap(),
        "{\"to\":\"\",\"pause\":true}\n{\"to\":\"\",\"pause\":false}\n"
    );
}

#[test]
fn manual_pause_beats_exemption() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("transition.stdin");
    let st = Arc::new(state());
    st.set_session_paused(11, true);
    st.begin_apply();
    let (child, stdin) = capture_child(&output);
    let pid = child.id();
    st.restore_paper((child, stdin));
    st.allow_session_rendering_for(pid, Duration::from_secs(1));
    st.end_apply();
    st.set_paused(true);
    st.set_paused(false);
    st.set_session_paused(11, false);
    let (mut child, stdin) = st.take_paper().unwrap();
    drop(stdin);
    let _ = child.wait();
    assert_eq!(
        std::fs::read_to_string(&output).unwrap(),
        "{\"to\":\"\",\"pause\":true}\n{\"to\":\"\",\"pause\":false}\n"
    );
}

#[test]
fn tracked_child_inherits_pause() {
    let st = state();
    st.set_session_paused(11, true);
    let child = dummy();
    let pid = child.id();
    st.track(child);
    assert_eq!(wait_process_state(pid, |state| state == 'T'), Some('T'));
    st.set_session_paused(11, false);
    assert_ne!(wait_process_state(pid, |state| state != 'T'), Some('T'));
    st.kill_holders();
}

#[test]
fn still_swap_needs_stdin() {
    let st = state();
    assert!(!st.output_still_swap("DP-1", "/w/x.png", "fill"));
    st.set_output_still("DP-1", dummy(), None);
    assert!(!st.output_still_swap("DP-1", "/w/x.png", "fill"));
    st.kill_output_stills();
}

#[test]
fn slide_preload_lines() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("dp1.stdin");
    let st = state();
    let (child, stdin) = capture_child(&out);
    st.set_output_still("DP-1", child, stdin);
    assert!(st.output_still_swap_slide("DP-1", "/w/n.png", "up", 300, "fill"));
    assert!(st.output_still_preload("DP-1", vec!["/w/a.png".into(), "/w/b.png".into()], "fill",));
    assert!(!st.output_still_preload("DP-1", Vec::new(), "fill"));
    assert!(!st.output_still_preload("DP-9", vec!["/w/a.png".into()], "fill"));
    for (mut child, stdin) in st.take_all_output_stills() {
        drop(stdin);
        let _ = child.wait();
    }
    assert_eq!(
        json_lines(&out),
        vec![
            serde_json::json!({"path": "/w/n.png", "slide": "up", "duration_ms": 300, "fill": "fill"}),
            serde_json::json!({"path": "", "preload": ["/w/a.png", "/w/b.png"], "fill": "fill"}),
        ],
    );
}

#[test]
fn fleet_reaps_exited() {
    let st = state();
    let (dead, _stdin) = exited_child();
    st.track(dead);
    assert_eq!(st.fleet_len(), 0);
    st.track(dummy());
    let (dead2, _stdin2) = exited_child();
    st.track(dead2);
    assert_eq!(st.fleet_len(), 1);
    st.kill_all();
}

#[test]
fn we_alive_requires_live_process() {
    let st = state();
    let (dead, _stdin) = exited_child();
    st.track(dead);
    assert!(!st.renderer_alive("we"));
    st.track(dummy());
    assert!(st.renderer_alive("we"));
    st.kill_all();
}

#[test]
fn renderer_alive_per_type() {
    let st = state();
    for ty in ["video", "static", "we", "bogus"] {
        assert!(!st.renderer_alive(ty));
    }
    st.set_base_still(dummy(), None);
    st.set_video_paper("DP-1", dummy(), None);
    st.track(dummy());
    assert!(st.renderer_alive("static"));
    assert!(st.renderer_alive("video"));
    assert!(st.renderer_alive("we"));
    st.kill_all();
    for ty in ["video", "static", "we"] {
        assert!(!st.renderer_alive(ty));
    }
    let (child, _stdin) = exited_child();
    st.set_base_still(child, None);
    assert!(!st.renderer_alive("static"));
}

#[test]
fn dead_video_reads_gone() {
    let st = state();
    let (dead, dead_stdin) = exited_child();
    st.set_video_paper("DP-1", dead, dead_stdin);
    st.set_video_paper("DP-2", dummy(), None);
    assert!(!st.has_video_paper("DP-1"));
    assert!(st.has_video_paper("DP-2"));
    st.kill_video_papers();
}

#[test]
fn dead_still_reads_gone() {
    let st = state();
    let (dead, dead_stdin) = exited_child();
    st.set_output_still("DP-1", dead, dead_stdin);
    st.set_output_still("DP-2", dummy(), None);
    assert!(!st.has_output_still("DP-1"));
    assert!(st.has_output_still("DP-2"));
    st.kill_output_stills();
}

#[test]
fn noop_gate_needs_all_alive() {
    let st = state();
    st.set_video_paper("DP-1", dummy(), None);
    st.set_video_paper("DP-2", dummy(), None);
    assert!(st.renderer_alive("video"));
    let (dead, _) = exited_child();
    st.set_video_paper("DP-2", dead, None);
    assert!(!st.renderer_alive("video"));
    st.kill_video_papers();
}
