use super::{ApplyPhase, ApplyRuntime};

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
fn distinct_targets_survive_commits() {
    let runtime = ApplyRuntime::default();
    let first = runtime.intent("work", "DP-1");
    let second = runtime.intent("work", "DP-2");
    let other = runtime.intent("home", "DP-1");
    assert!(runtime.claim_intent(&first).is_some());
    assert!(runtime.claim_intent(&second).is_some());
    assert!(runtime.claim_intent(&other).is_some());
    assert!(runtime.claim_intent(&first).is_none());
}

#[test]
fn overlapping_targets_supersede() {
    let runtime = ApplyRuntime::default();
    let old = runtime.intent("work", "DP-1");
    let latest = runtime.intent("work", "DP-1");
    assert!(!runtime.intent_current(&old));
    assert!(runtime.intent_current(&latest));
    let all = runtime.intent("work", "*");
    assert!(!runtime.intent_current(&latest));
    let one = runtime.intent("work", "DP-2");
    assert!(!runtime.intent_current(&all));
    runtime.next_generation();
    assert!(!runtime.intent_current(&one));
}

#[test]
fn phases_sum_per_recording() {
    let runtime = ApplyRuntime::default();
    let recording = runtime.record_phases();
    let phases = [
        (ApplyPhase::Lock, 7),
        (ApplyPhase::Prepare, 11),
        (ApplyPhase::ReconcileTotal, 13),
        (ApplyPhase::Capture, 17),
        (ApplyPhase::Ready, 40),
        (ApplyPhase::Ready, 2),
    ];
    for (phase, millis) in phases {
        runtime.note_phase(phase, std::time::Duration::from_millis(millis));
    }

    assert_eq!(
        runtime.phases().to_string(),
        "lock_ms=7 prep_ms=11 reconcile_total_ms=13 capture_ms=17 ready_ms=42"
    );
    drop(recording);
    assert_eq!(
        runtime.phases().to_string(),
        "lock_ms=0 prep_ms=0 reconcile_total_ms=0 capture_ms=0 ready_ms=0"
    );
}

#[test]
fn phases_ignored_unrecorded() {
    let runtime = ApplyRuntime::default();
    runtime.note_phase(ApplyPhase::Lock, std::time::Duration::from_millis(9));
    let _recording = runtime.record_phases();

    assert_eq!(
        runtime.phases().to_string(),
        "lock_ms=0 prep_ms=0 reconcile_total_ms=0 capture_ms=0 ready_ms=0"
    );
}
