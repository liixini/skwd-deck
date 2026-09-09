use super::*;

#[test]
fn only_visible_fullscreen_outputs_block_playback() {
    let mut monitor = Monitor { supported: true, ..Monitor::default() };
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
    let mut monitor = Monitor { supported: true, ..Monitor::default() };
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
