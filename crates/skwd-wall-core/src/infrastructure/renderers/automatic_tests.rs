use super::supervisor::{PausePolicy, RendererSupervisor, capture_child};
use std::collections::HashSet;

#[test]
fn automatic_reasons_compose_with_manual_and_session_pause() {
    let policy = PausePolicy {
        manual: false,
        manual_outputs: std::collections::HashMap::new(),
        session: false,
        automatic: false,
        automatic_outputs: HashSet::from(["DP-1".into()]),
    };
    assert!(policy.paused_for(false, "DP-1"));
    assert!(!policy.paused_for(false, "DP-2"));
    assert!(!policy.paused_for(false, "DP-1,DP-2"));
    let manual = PausePolicy { manual: true, ..policy.clone() };
    assert!(manual.paused_for(true, "DP-2"));
    let auto = PausePolicy { automatic: true, ..policy };
    assert!(auto.paused_for(true, "DP-2"));
}

#[test]
fn an_automatic_pause_survives_manual_resume_and_new_renderer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pause");
    let supervisor = RendererSupervisor::default();
    supervisor.set_automatic_paused(true, HashSet::new());
    let (child, stdin) = capture_child(&path);
    supervisor.set_video_paper("DP-1", child, stdin);
    supervisor.set_paused(true);
    supervisor.set_automatic_paused(false, HashSet::new());
    assert!(supervisor.paused());
    supervisor.set_paused(false);
    std::thread::sleep(std::time::Duration::from_millis(40));
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "{\"to\":\"\",\"pause\":true}\n{\"to\":\"\",\"pause\":false}\n"
    );
}

#[test]
fn monitor_pause_is_independent_and_preserves_other_reasons() {
    let dir = tempfile::tempdir().unwrap();
    let supervisor = RendererSupervisor::default();
    for output in ["DP-1", "DP-2"] {
        let (child, stdin) = capture_child(&dir.path().join(output));
        supervisor.set_video_paper(output, child, stdin);
    }
    supervisor.set_output_paused("DP-1", true);
    assert!(supervisor.paused_for("DP-1"));
    assert!(!supervisor.paused_for("DP-2"));
    supervisor.set_automatic_paused(false, HashSet::from(["DP-1".into()]));
    supervisor.set_output_paused("DP-1", false);
    assert!(supervisor.paused_for("DP-1"));
    assert!(!supervisor.manual_paused_for("DP-1"));
    supervisor.set_automatic_paused(false, HashSet::new());
    supervisor.set_paused(true);
    supervisor.set_output_paused("DP-1", false);
    assert!(!supervisor.paused_for("DP-1"));
    assert!(supervisor.paused_for("DP-2"));
    supervisor.set_paused(false);
    std::thread::sleep(std::time::Duration::from_millis(50));
    let lines = |name| {
        std::fs::read_to_string(dir.path().join(name))
            .unwrap()
            .lines()
            .map(|line| {
                serde_json::from_str::<serde_json::Value>(line).unwrap()["pause"].as_bool().unwrap()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(lines("DP-1"), [true, false, true, false]);
    assert_eq!(lines("DP-2"), [true, false]);
}

#[test]
fn audio_duck_reaches_live_and_new_renderers_without_touching_pause() {
    let dir = tempfile::tempdir().unwrap();
    let supervisor = RendererSupervisor::default();
    let (child, stdin) = capture_child(&dir.path().join("live"));
    supervisor.set_video_paper("DP-1", child, stdin);
    assert!(!supervisor.audio_ducked());
    supervisor.set_audio_ducked(true);
    supervisor.set_audio_ducked(true);
    assert!(supervisor.audio_ducked());
    assert!(!supervisor.paused());
    let (child, stdin) = capture_child(&dir.path().join("new"));
    supervisor.restore_video_paper("DP-2", (child, stdin));
    supervisor.set_audio_ducked(false);
    std::thread::sleep(std::time::Duration::from_millis(40));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("live")).unwrap(),
        "{\"to\":\"\",\"duck\":true}\n{\"to\":\"\",\"duck\":false}\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("new")).unwrap(),
        "{\"to\":\"\",\"duck\":true}\n{\"to\":\"\",\"duck\":false}\n"
    );
}
