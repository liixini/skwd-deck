#![cfg(feature = "loom")]

use loom::sync::{Arc, Condvar, Mutex};
use loom::thread;
use std::collections::HashMap;

struct ReadyGate {
    done: Mutex<bool>,
    cv: Condvar,
}

type Registry = Mutex<HashMap<u32, Arc<ReadyGate>>>;

fn ready_gate(reg: &Registry, pid: u32) -> Arc<ReadyGate> {
    let mut map = reg.lock().unwrap();
    map.entry(pid)
        .or_insert_with(|| Arc::new(ReadyGate { done: Mutex::new(false), cv: Condvar::new() }))
        .clone()
}

fn ready_signal(reg: &Registry, pid: u32) {
    let gate = ready_gate(reg, pid);
    *gate.done.lock().unwrap() = true;
    gate.cv.notify_all();
}

fn ready_wait(reg: &Registry, pid: u32) -> bool {
    let gate = ready_gate(reg, pid);
    let mut done = gate.done.lock().unwrap();
    while !*done {
        done = gate.cv.wait(done).unwrap();
    }
    let got = *done;
    drop(done);
    reg.lock().unwrap().remove(&pid);
    got
}

#[test]
fn signal_races_wait() {
    loom::model(|| {
        let reg: Arc<Registry> = Arc::new(Mutex::new(HashMap::new()));
        let signaller = {
            let reg = reg.clone();
            thread::spawn(move || ready_signal(&reg, 7))
        };
        let got = ready_wait(&reg, 7);
        signaller.join().unwrap();
        assert!(got);
        assert!(reg.lock().unwrap().is_empty());
    });
}

#[test]
fn signal_before_wait() {
    loom::model(|| {
        let reg: Registry = Mutex::new(HashMap::new());
        ready_signal(&reg, 3);
        assert!(ready_wait(&reg, 3));
        assert!(reg.lock().unwrap().is_empty());
    });
}
