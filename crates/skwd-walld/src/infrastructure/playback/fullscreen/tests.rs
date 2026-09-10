use super::*;

#[test]
fn unavailable_or_disabled_detection_retains_native_policy_delivery() {
    let native =
        Snapshot { observed_outputs: HashSet::from(["DP-1".into()]), ..Snapshot::default() };
    for enabled in [false, true] {
        let selected = selected_snapshot(enabled, &Snapshot::default(), &native);
        assert!(!selected.supported);
        assert!(selected.outputs.is_empty());
        assert_eq!(selected.observed_outputs, native.observed_outputs);
    }
}

#[test]
fn protocol_without_output_names_is_not_usable_detection() {
    let monitor = Monitor { backends: HashMap::from([(Backend::Wlr, 42)]), ..Monitor::default() };
    assert!(!monitor.snapshot().supported);
    assert!(!monitor.snapshot().maximized_supported);
}

#[test]
fn incomplete_state_batches_never_change_pause_outputs() {
    let mut monitor =
        Monitor { backends: HashMap::from([(Backend::Wlr, 42)]), ..Monitor::default() };
    monitor.outputs.insert(1, "DP-1".into());
    monitor
        .pending
        .insert(8, Window { fullscreen: true, outputs: HashSet::from([1]), ..Window::default() });
    assert!(monitor.snapshot().outputs.is_empty());
    monitor.commit(8);
    assert!(monitor.snapshot().outputs.contains("DP-1"));
    monitor.pending.get_mut(&8).unwrap().fullscreen = false;
    assert!(monitor.snapshot().outputs.contains("DP-1"));
    monitor.commit(8);
    assert!(monitor.snapshot().outputs.is_empty());
}

#[test]
fn cosmic_windows_follow_active_workspaces_and_override_duplicate_wlr_events() {
    let mut monitor = Monitor {
        backends: HashMap::from([(Backend::Wlr, 42), (Backend::Cosmic, 43)]),
        ..Monitor::default()
    };
    monitor.outputs.insert(1, "DP-1".into());
    monitor.active_workspaces.insert(20);
    monitor
        .windows
        .insert(8, Window { fullscreen: true, outputs: HashSet::from([1]), ..Window::default() });
    monitor.windows.insert(
        9,
        Window {
            backend: Backend::Cosmic,
            maximized: true,
            outputs: HashSet::from([1]),
            workspaces: HashSet::from([21]),
            ..Window::default()
        },
    );
    assert!(monitor.snapshot().outputs.is_empty());
    assert!(monitor.snapshot().maximized_outputs.is_empty());
    monitor.active_workspaces.insert(21);
    assert!(monitor.snapshot().maximized_outputs.contains("DP-1"));
    monitor.windows.get_mut(&9).unwrap().minimized = true;
    assert!(monitor.snapshot().maximized_outputs.is_empty());
    monitor.remove(9);
    assert!(monitor.snapshot().outputs.is_empty());
}

#[test]
fn only_visible_fullscreen_outputs_block_playback() {
    let mut monitor =
        Monitor { backends: HashMap::from([(Backend::Wlr, 42)]), ..Monitor::default() };
    monitor.outputs.insert(1, "DP-1".into());
    monitor.outputs.insert(2, "DP-2".into());
    monitor
        .windows
        .insert(8, Window { fullscreen: true, outputs: HashSet::from([1]), ..Window::default() });
    assert_eq!(monitor.snapshot().outputs, HashSet::from(["DP-1".into()]));
    monitor.windows.get_mut(&8).unwrap().minimized = true;
    assert!(monitor.snapshot().outputs.is_empty());
    monitor.windows.get_mut(&8).unwrap().minimized = false;
    monitor.windows.get_mut(&8).unwrap().outputs.clear();
    assert!(monitor.snapshot().outputs.is_empty());
}

#[tokio::test]
#[ignore = "requires the user's Wayland compositor"]
async fn live_fullscreen_protocol() {
    let (enabled, receive_enabled) = watch::channel(true);
    let (send, mut receive) = watch::channel(Snapshot::default());
    let task = tokio::spawn(run(receive_enabled, send));
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if receive.borrow().supported {
                break;
            }
            receive.changed().await.unwrap();
        }
    })
    .await
    .expect("compositor advertises fullscreen and output events");
    println!("Live fullscreen snapshot: {:?}", receive.borrow().clone());
    enabled.send(false).unwrap();
    task.abort();
}

#[test]
fn maximized_windows_are_tracked_separately_and_clear_when_hidden_or_closed() {
    let mut monitor =
        Monitor { backends: HashMap::from([(Backend::Wlr, 42)]), ..Monitor::default() };
    monitor.outputs.insert(1, "DP-1".into());
    monitor.outputs.insert(2, "DP-2".into());
    monitor
        .windows
        .insert(8, Window { maximized: true, outputs: HashSet::from([1]), ..Window::default() });
    monitor
        .windows
        .insert(9, Window { fullscreen: true, outputs: HashSet::from([2]), ..Window::default() });
    assert_eq!(monitor.snapshot().maximized_outputs, HashSet::from(["DP-1".into()]));
    assert_eq!(monitor.snapshot().outputs, HashSet::from(["DP-2".into()]));
    monitor.windows.get_mut(&8).unwrap().outputs = HashSet::from([2]);
    assert_eq!(monitor.snapshot().maximized_outputs, HashSet::from(["DP-2".into()]));
    monitor.windows.get_mut(&8).unwrap().minimized = true;
    assert!(monitor.snapshot().maximized_outputs.is_empty());
    monitor.windows.get_mut(&8).unwrap().minimized = false;
    monitor.windows.get_mut(&8).unwrap().outputs.clear();
    assert!(monitor.snapshot().maximized_outputs.is_empty());
    monitor.windows.get_mut(&8).unwrap().outputs.insert(1);
    monitor.windows.get_mut(&8).unwrap().maximized = false;
    assert!(monitor.snapshot().maximized_outputs.is_empty());
    monitor.windows.get_mut(&8).unwrap().maximized = true;
    monitor.windows.remove(&8);
    assert!(monitor.snapshot().maximized_outputs.is_empty());
    assert_eq!(monitor.snapshot().outputs, HashSet::from(["DP-2".into()]));
}
