use std::sync::{Arc, Mutex, Weak};

use paper_control::{
    ClockStyle, Depth, Face, Labels, Palette, PaperCommand, Pin, StillCommand, Widget, WidgetScene,
};

use super::{RendererSupervisor, WidgetSource};
use crate::infrastructure::renderers::{capture_child, exited_child};

#[derive(Default)]
struct Fixed {
    widgets: Mutex<Vec<Widget>>,
    seen: Mutex<Vec<String>>,
}

impl WidgetSource for Fixed {
    fn scene(&self, shown: &[String]) -> WidgetScene {
        *self.seen.lock().unwrap() = shown.to_vec();
        WidgetScene { widgets: self.widgets.lock().unwrap().clone(), ..scene() }
    }
}

struct Probe {
    supervisor: Weak<RendererSupervisor>,
    held: Mutex<Vec<(bool, bool)>>,
}

impl WidgetSource for Probe {
    fn scene(&self, _: &[String]) -> WidgetScene {
        let supervisor = self.supervisor.upgrade().unwrap();
        let stdin = supervisor.still_stdin.try_lock().is_err();
        let sync = supervisor.widget_sync.try_lock().is_err();
        self.held.lock().unwrap().push((stdin, sync));
        WidgetScene { widgets: vec![clock()], ..scene() }
    }
}

fn scene() -> WidgetScene {
    WidgetScene {
        widgets: Vec::new(),
        outlines: Vec::new(),
        palette: Palette { text: [1, 2, 3], accent: [4, 5, 6], shade: [7, 8, 9] },
        font: None,
        labels: Labels {
            days: std::array::from_fn(|_| String::new()),
            day_names: std::array::from_fn(|_| String::new()),
            months: std::array::from_fn(|_| String::new()),
            month_names: std::array::from_fn(|_| String::new()),
        },
    }
}

fn clock() -> Widget {
    Widget {
        face: Face::Clock(ClockStyle::Aperture),
        depth: Depth::Front,
        pin: Pin { anchor: [0.5, 0.5], offset: [0.0, 0.0] },
        height: None,
        output: None,
    }
}

fn supervised(widgets: Vec<Widget>) -> (RendererSupervisor, Arc<Fixed>) {
    let supervisor = RendererSupervisor::default();
    let source = Arc::new(Fixed { widgets: Mutex::new(widgets), ..Fixed::default() });
    supervisor.set_widget_source(Arc::clone(&source) as Arc<dyn WidgetSource>);
    (supervisor, source)
}

fn written(supervisor: &RendererSupervisor, out: &std::path::Path) -> usize {
    let (mut child, stdin) = supervisor.take_base_still().unwrap();
    drop(stdin);
    let _ = child.wait();
    std::fs::read_to_string(out).unwrap().lines().count()
}

#[test]
fn source_sees_shown() {
    let (supervisor, source) = supervised(vec![clock()]);
    supervisor.set_assignment("DP-1", "/w/b.png");
    supervisor.set_assignment("DP-2", "/w/a.png");
    let command = supervisor.with_widgets(StillCommand::new("/w/c.png"));
    assert_eq!(command.widgets.unwrap().widgets, [clock()]);
    assert_eq!(*source.seen.lock().unwrap(), ["/w/a.png", "/w/b.png", "/w/c.png"]);
}

#[test]
fn empty_path_unchanged() {
    let (supervisor, _) = supervised(vec![clock()]);
    assert_eq!(supervisor.with_widgets(StillCommand::new("")).widgets, None);
}

#[test]
fn bare_scene_unattached() {
    let (supervisor, _) = supervised(Vec::new());
    assert_eq!(supervisor.with_widgets(StillCommand::new("/w/a.png")).widgets, None);
}

#[test]
fn no_source_no_scene() {
    let supervisor = RendererSupervisor::default();
    assert_eq!(supervisor.with_widgets(StillCommand::new("/w/a.png")).widgets, None);
    assert!(supervisor.refresh_widgets().is_ok());
}

#[test]
fn editing_hides_widgets() {
    let (supervisor, _) = supervised(vec![clock()]);
    assert!(supervisor.widget_editor(true));
    assert!(!supervisor.widget_editor(true));
    assert!(!supervisor.widget_editor(false));
    assert!(supervisor.widget_scene(None).unwrap().widgets.is_empty());
    assert_eq!(supervisor.committed_widgets(None).unwrap().widgets, [clock()]);
    assert!(supervisor.widget_editor(false));
    assert!(!supervisor.widget_editor(false));
    assert_eq!(supervisor.widget_scene(None).unwrap().widgets, [clock()]);
}

#[test]
fn refresh_skips_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("still.stdin");
    let (supervisor, source) = supervised(vec![clock()]);
    let (child, stdin) = capture_child(&out);
    supervisor.set_base_still(child, stdin);
    supervisor.refresh_widgets().unwrap();
    supervisor.refresh_widgets().unwrap();
    source.widgets.lock().unwrap().clear();
    supervisor.refresh_widgets().unwrap();
    supervisor.refresh_widgets().unwrap();
    assert_eq!(written(&supervisor, &out), 2);
}

#[test]
fn refresh_skips_bare() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("still.stdin");
    let (supervisor, _) = supervised(Vec::new());
    let (child, stdin) = capture_child(&out);
    supervisor.set_base_still(child, stdin);
    supervisor.refresh_widgets().unwrap();
    supervisor.refresh_widgets().unwrap();
    assert_eq!(written(&supervisor, &out), 1);
}

#[test]
fn editing_after_start() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("still.stdin");
    let (supervisor, _) = supervised(vec![clock()]);
    let (child, stdin) = capture_child(&out);
    supervisor.set_base_still(child, stdin);
    assert!(supervisor.still_swap("/w/a.png", "fill"));
    supervisor.widget_editor(true);
    supervisor.refresh_widgets().unwrap();
    let (mut child, stdin) = supervisor.take_base_still().unwrap();
    drop(stdin);
    let _ = child.wait();
    let lines: Vec<StillCommand> = std::fs::read_to_string(&out)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let scenes: Vec<usize> = lines
        .iter()
        .filter_map(|command| command.widgets.as_ref())
        .map(|scene| scene.widgets.len())
        .collect();
    assert_eq!(scenes, [1, 0]);
}

#[test]
fn failed_send_retries() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("still.stdin");
    let (supervisor, _) = supervised(vec![clock()]);
    let (child, stdin) = exited_child();
    supervisor.set_base_still(child, stdin);
    assert!(supervisor.refresh_widgets().is_err());
    let (child, stdin) = capture_child(&out);
    supervisor.set_base_still(child, stdin);
    supervisor.refresh_widgets().unwrap();
    assert_eq!(written(&supervisor, &out), 1);
}

#[test]
fn scene_lock_order() {
    let dir = tempfile::tempdir().unwrap();
    let supervisor = Arc::new(RendererSupervisor::default());
    let probe = Arc::new(Probe { supervisor: Arc::downgrade(&supervisor), held: Mutex::default() });
    supervisor.set_widget_source(Arc::clone(&probe) as Arc<dyn WidgetSource>);
    let (child, stdin) = capture_child(&dir.path().join("still.stdin"));
    supervisor.set_base_still(child, stdin);
    assert!(supervisor.still_swap("/w/a.png", "fill"));
    supervisor.greet_still(None, "/w/a.png");
    supervisor.refresh_widgets().unwrap();
    assert_eq!(*probe.held.lock().unwrap(), [(true, false), (true, false), (false, true)]);
    let (mut child, stdin) = supervisor.take_base_still().unwrap();
    drop(stdin);
    let _ = child.wait();
}

#[test]
fn video_widgets_follow_changes() {
    for key in ["DP-1", "DP-1,DP-2", "multi", "*"] {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("video.stdin");
        let (supervisor, source) = supervised(vec![clock()]);
        let (child, stdin) = capture_child(&out);
        supervisor.set_video_paper(key, child, stdin);
        supervisor.greet_dynamic(Some(key));
        source.widgets.lock().unwrap()[0].pin.offset = [12.0, 0.0];
        supervisor.refresh_widgets().unwrap();
        supervisor.refresh_widgets().unwrap();
        supervisor.widget_editor(true);
        supervisor.refresh_widgets().unwrap();
        let (mut child, stdin) = supervisor.take_video_paper(key).unwrap();
        drop(stdin);
        let _ = child.wait();
        let commands: Vec<PaperCommand> = std::fs::read_to_string(&out)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let scenes: Vec<_> =
            commands.iter().filter_map(|command| command.widgets.as_ref()).collect();
        assert_eq!(scenes.len(), 3);
        assert_eq!(scenes[0].widgets, [clock()]);
        assert_eq!(scenes[1].widgets[0].pin.offset, [12.0, 0.0]);
        assert!(scenes[2].widgets.is_empty());
    }
}

#[test]
fn transition_widgets_follow_changes() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("transition.stdin");
    let (supervisor, source) = supervised(vec![clock()]);
    let (child, stdin) = capture_child(&out);
    supervisor.restore_paper((child, stdin));
    supervisor.greet_dynamic(None);
    source.widgets.lock().unwrap().clear();
    supervisor.refresh_widgets().unwrap();
    let (mut child, stdin) = supervisor.take_paper().unwrap();
    drop(stdin);
    let _ = child.wait();
    let commands: Vec<PaperCommand> = std::fs::read_to_string(&out)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(commands[0].widgets.as_ref().unwrap().widgets, [clock()]);
    assert!(commands[1].widgets.as_ref().unwrap().widgets.is_empty());
}
