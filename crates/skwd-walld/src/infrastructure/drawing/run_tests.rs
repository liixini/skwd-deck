use super::*;
use serde_json::json;

fn child(script: &str) -> Child {
    skwd_wall_core::proc::tool("sh").args(["-c", script]).stdout(Stdio::piped()).spawn().unwrap()
}

#[test]
fn waits_after_stdout_closes() {
    let (_guard, _) = crate::testenv::lock();
    crate::testenv::write_config(json!({"drawing":{"onBattery":true}}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let worker = child("exec 1>&-; exec sleep 0.2");
    assert!(supervise(&ctx, worker, u64::MAX, &AtomicBool::new(false), Instant::now()).is_ok());
}

#[test]
fn cancellation_reaps_worker() {
    let (_guard, _) = crate::testenv::lock();
    crate::testenv::write_config(json!({"drawing":{"onBattery":true}}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let worker = child("exec sleep 30");
    let pid = worker.id();
    assert!(supervise(&ctx, worker, u64::MAX, &AtomicBool::new(true), Instant::now()).is_err());
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
}

#[test]
fn rejects_failed_worker() {
    let (_guard, _) = crate::testenv::lock();
    crate::testenv::write_config(json!({"drawing":{"onBattery":true}}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    assert!(
        supervise(&ctx, child("exit 7"), u64::MAX, &AtomicBool::new(false), Instant::now())
            .is_err()
    );
}
