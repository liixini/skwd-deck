use std::collections::HashMap;
use std::process::{Child, ChildStdin};
use std::sync::Mutex;

use crate::lock;

pub(super) type ChildMap = HashMap<String, (Child, Option<ChildStdin>)>;

pub(crate) fn kill_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

pub(super) fn slot_alive(slot: &Mutex<Option<Child>>) -> bool {
    let mut guard = lock(slot);
    match guard.as_mut() {
        Some(child) => matches!(child.try_wait(), Ok(None)),
        None => false,
    }
}

pub(super) fn slot_pid_alive(slot: &Mutex<Option<Child>>, pid: u32) -> bool {
    let mut guard = lock(slot);
    match guard.as_mut() {
        Some(child) if child.id() == pid => matches!(child.try_wait(), Ok(None)),
        Some(_) | None => false,
    }
}

pub(super) fn map_all_alive(map: &Mutex<ChildMap>) -> bool {
    let mut guard = lock(map);
    !guard.is_empty() && guard.values_mut().all(|(child, _)| matches!(child.try_wait(), Ok(None)))
}

pub(super) fn map_entry_alive(map: &Mutex<ChildMap>, output: &str) -> bool {
    let mut guard = lock(map);
    match guard.get_mut(output) {
        Some((child, _)) => matches!(child.try_wait(), Ok(None)),
        None => false,
    }
}

pub(super) fn map_entry_pid_alive(map: &Mutex<ChildMap>, output: &str, pid: u32) -> bool {
    let mut guard = lock(map);
    match guard.get_mut(output) {
        Some((child, _)) if child.id() == pid => matches!(child.try_wait(), Ok(None)),
        Some(_) | None => false,
    }
}

pub(super) fn replace(map: &mut ChildMap, output: &str, child: Child, stdin: Option<ChildStdin>) {
    if let Some((mut old, _)) = map.remove(output) {
        kill_child(&mut old);
    }
    map.insert(output.to_string(), (child, stdin));
}

pub(super) fn retain(map: &mut ChildMap, keep: &[String]) {
    let remove: Vec<String> = map.keys().filter(|key| !keep.contains(key)).cloned().collect();
    for key in remove {
        if let Some((mut child, _)) = map.remove(&key) {
            kill_child(&mut child);
        }
    }
}

pub(super) fn reap(map: &mut ChildMap) -> Vec<String> {
    let mut exited = Vec::new();
    map.retain(|output, (child, _)| match child.try_wait() {
        Ok(Some(_)) => {
            exited.push(output.clone());
            false
        }
        Ok(None) | Err(_) => true,
    });
    exited
}

pub(super) fn kill_all(map: &mut ChildMap) {
    for (_, (mut child, _)) in map.drain() {
        kill_child(&mut child);
    }
}

pub(super) fn kill_one(map: &mut ChildMap, output: &str) {
    if let Some((mut child, _)) = map.remove(output) {
        kill_child(&mut child);
    }
}
