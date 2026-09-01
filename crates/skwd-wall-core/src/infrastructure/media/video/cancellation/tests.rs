use std::time::{Duration, Instant};

use super::{Cancellation, CancellationState, WakeReason};

#[test]
fn replacement_invalidates_the_active_generation() {
    let cancellation = Cancellation::new();
    let active = cancellation.snapshot();
    cancellation.replace();
    assert!(cancellation.replaced(active));
    assert_eq!(cancellation.state(active), CancellationState::Replaced);
    assert_eq!(
        cancellation.wait_until(active, Instant::now() + Duration::from_secs(1)),
        WakeReason::Replaced
    );
}

#[test]
fn closure_cancels_waiters_and_marks_the_lifecycle_closed() {
    let cancellation = Cancellation::new();
    let active = cancellation.snapshot();
    cancellation.close();
    assert!(cancellation.closed());
    assert!(cancellation.replaced(active));
    assert_eq!(cancellation.state(active), CancellationState::Closed);
    assert_eq!(
        cancellation.wait_until(active, Instant::now() + Duration::from_secs(1)),
        WakeReason::Closed
    );
}

#[test]
fn unchanged_generation_reaches_its_deadline() {
    let cancellation = Cancellation::new();
    assert_eq!(
        cancellation.wait_until(cancellation.snapshot(), Instant::now()),
        WakeReason::Deadline
    );
}
