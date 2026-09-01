use super::ApplyRuntime;

#[test]
fn concurrent_apply_owners_are_serialized() {
    let runtime = std::sync::Arc::new(ApplyRuntime::default());
    let incumbent = runtime.lock();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (acquired_tx, acquired_rx) = std::sync::mpsc::channel();
    let contender = runtime.clone();
    let join = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        let _ownership = contender.lock();
        acquired_tx.send(()).unwrap();
    });

    started_rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
    assert!(
        acquired_rx.recv_timeout(std::time::Duration::from_millis(50)).is_err(),
        "a second apply must not enter while the incumbent owns orchestration"
    );
    drop(incumbent);
    acquired_rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
    join.join().unwrap();
}

#[test]
fn swap_slide_one_shot() {
    let runtime = ApplyRuntime::default();

    runtime.set_swap_slide(Some(("up".to_string(), 300)));
    assert_eq!(runtime.take_swap_slide(), Some(("up".to_string(), 300)));
    assert_eq!(runtime.take_swap_slide(), None);
}

#[test]
fn generation_monotonic() {
    let runtime = ApplyRuntime::default();

    assert_eq!(runtime.next_generation(), 1);
    assert_eq!(runtime.next_generation(), 2);
    assert_eq!(runtime.generation(), 2);
}

#[test]
fn claim_current_generation() {
    let runtime = ApplyRuntime::default();
    let queued = runtime.next_generation();
    assert_eq!(runtime.claim_generation(queued), Some(queued + 1));
    assert_eq!(runtime.claim_generation(queued), None);
    assert_eq!(runtime.generation(), queued + 1);
}
