use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Instant;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum WakeReason {
    Replaced,
    Closed,
    Deadline,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CancellationState {
    Active,
    Replaced,
    Closed,
}

pub(super) struct Cancellation {
    generation: AtomicU64,
    closed: AtomicBool,
    wait: Mutex<()>,
    changed: Condvar,
}

impl Cancellation {
    pub(super) fn new() -> Self {
        Self {
            generation: AtomicU64::new(0),
            closed: AtomicBool::new(false),
            wait: Mutex::new(()),
            changed: Condvar::new(),
        }
    }

    pub(super) fn snapshot(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    pub(super) fn replaced(&self, snapshot: u64) -> bool {
        self.snapshot() != snapshot
    }

    pub(super) fn state(&self, snapshot: u64) -> CancellationState {
        if self.closed() {
            CancellationState::Closed
        } else if self.replaced(snapshot) {
            CancellationState::Replaced
        } else {
            CancellationState::Active
        }
    }

    pub(super) fn replace(&self) {
        let _guard = self.wait.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.changed.notify_all();
    }

    pub(super) fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.replace();
    }

    pub(super) fn closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    pub(super) fn wait_until(&self, snapshot: u64, deadline: Instant) -> WakeReason {
        let guard = self.wait.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let _guard = self
            .changed
            .wait_timeout_while(guard, deadline.saturating_duration_since(Instant::now()), |()| {
                !self.closed() && !self.replaced(snapshot)
            })
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .0;
        match self.state(snapshot) {
            CancellationState::Closed => WakeReason::Closed,
            CancellationState::Replaced => WakeReason::Replaced,
            CancellationState::Active => WakeReason::Deadline,
        }
    }
}

#[cfg(test)]
mod tests;
