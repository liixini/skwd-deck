use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::lock;

#[derive(Default)]
struct ReadyGate {
    done: Mutex<bool>,
    changed: Condvar,
}

#[derive(Default)]
pub(super) struct ReadinessRegistry {
    gates: Mutex<HashMap<u32, (Arc<ReadyGate>, Instant)>>,
}

pub struct ReadyWaiter {
    gate: Arc<ReadyGate>,
}

impl ReadyWaiter {
    pub fn wait(self, timeout: Duration) -> bool {
        let done = lock(&self.gate.done);
        let (done, _) = self
            .gate
            .changed
            .wait_timeout_while(done, timeout, |ready| !*ready)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *done
    }
}

const GATE_TTL: Duration = Duration::from_secs(60);

impl ReadinessRegistry {
    fn gate(&self, pid: u32) -> Arc<ReadyGate> {
        let mut gates = lock(&self.gates);
        gates.retain(|_, (_, born)| born.elapsed() < GATE_TTL);
        gates
            .entry(pid)
            .or_insert_with(|| (Arc::new(ReadyGate::default()), Instant::now()))
            .0
            .clone()
    }

    pub(super) fn signal(&self, pid: u32) {
        let gate = self.gate(pid);
        *lock(&gate.done) = true;
        gate.changed.notify_all();
    }

    pub(super) fn arm(&self, pid: u32) {
        lock(&self.gates).remove(&pid);
    }

    pub(super) fn cancel(&self, pid: u32) {
        lock(&self.gates).remove(&pid);
    }

    pub(super) fn waiter(&self, pid: u32) -> ReadyWaiter {
        ReadyWaiter { gate: self.gate(pid) }
    }

    pub(super) fn wait(&self, pid: u32, timeout: Duration) -> bool {
        let gate = self.gate(pid);
        let done = lock(&gate.done);
        let (done, _) = gate
            .changed
            .wait_timeout_while(done, timeout, |ready| !*ready)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let ready = *done;
        drop(done);
        lock(&self.gates).remove(&pid);
        ready
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        lock(&self.gates).len()
    }

    #[cfg(test)]
    pub(super) fn expire_all(&self) {
        for (_, born) in lock(&self.gates).values_mut() {
            *born = Instant::now().checked_sub(GATE_TTL + Duration::from_secs(1)).unwrap();
        }
    }
}
