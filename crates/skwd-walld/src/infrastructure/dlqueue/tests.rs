#![cfg(test)]

use super::{Gate, queue_label};
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn free_gate_admits() {
    let gate = Gate::new(2);
    let first = gate.try_reserve().unwrap().acquire(|_| panic!("free gate waited"));
    let second = gate.try_reserve().unwrap().acquire(|_| panic!("second slot waited"));
    drop(first);
    drop(second);
}

#[test]
fn full_gate_fifo() {
    static GATE: Gate = Gate::new(1);
    let held = GATE.try_reserve().unwrap().acquire(|_| {});
    let (pos_tx, pos_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let mut workers = Vec::new();
    for idx in 0..2u64 {
        let (pos_tx, done_tx) = (pos_tx.clone(), done_tx.clone());
        workers.push(std::thread::spawn(move || {
            let slot =
                GATE.try_reserve().unwrap().acquire(|ahead| pos_tx.send((idx, ahead)).unwrap());
            done_tx.send(idx).unwrap();
            std::thread::sleep(Duration::from_millis(20));
            drop(slot);
        }));
        let (who, ahead) = pos_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!((who, ahead), (idx, idx));
    }
    drop(held);
    assert_eq!(done_rx.recv_timeout(Duration::from_secs(5)).unwrap(), 0);
    let (who, ahead) = pos_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!((who, ahead), (1, 0));
    assert_eq!(done_rx.recv_timeout(Duration::from_secs(5)).unwrap(), 1);
    for worker in workers {
        worker.join().unwrap();
    }
}

#[test]
fn release_admits_next() {
    let gate = Gate::new(1);
    let held = gate.try_reserve().unwrap().acquire(|_| {});
    std::thread::scope(|scope| {
        let (tx, rx) = mpsc::channel();
        let shared = &gate;
        scope.spawn(move || {
            let slot = shared.try_reserve().unwrap().acquire(|_| {});
            tx.send(()).unwrap();
            drop(slot);
        });
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
        drop(held);
        rx.recv_timeout(Duration::from_secs(5)).expect("release admits waiter");
    });
}

#[test]
fn reservation_bounds() {
    let gate = Gate::with_queue(1, 2);
    let active = gate.try_reserve().unwrap();
    let queued_a = gate.try_reserve().unwrap();
    let queued_b = gate.try_reserve().unwrap();
    assert!(gate.try_reserve().is_none());

    drop(queued_a);
    assert!(gate.try_reserve().is_some());
    drop(active);
    drop(queued_b);
}

#[test]
fn queue_label_text() {
    assert_eq!(queue_label(0), "queued");
    assert_eq!(queue_label(2), "queued - 2 ahead");
}
