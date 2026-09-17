use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde_json::{Map, Value};

#[derive(Default)]
struct Channel {
    subscribers: HashMap<String, usize>,
    entries: Map<String, Value>,
}

static CHANNEL: OnceLock<Mutex<Channel>> = OnceLock::new();
static NOTIFIER: OnceLock<fn()> = OnceLock::new();

fn channel() -> &'static Mutex<Channel> {
    CHANNEL.get_or_init(Mutex::default)
}

fn notify() {
    if let Some(notify) = NOTIFIER.get() {
        notify();
    }
}

pub fn set_notifier(notify: fn()) {
    let _ = NOTIFIER.set(notify);
}

#[must_use]
pub struct Subscription {
    output: String,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        let mut channel = crate::lock(channel());
        if let Some(count) = channel.subscribers.get_mut(&self.output) {
            *count -= 1;
            if *count == 0 {
                channel.subscribers.remove(&self.output);
            }
        }
    }
}

pub fn subscribe(output: &str) -> Subscription {
    *crate::lock(channel()).subscribers.entry(output.to_string()).or_default() += 1;
    Subscription { output: output.to_string() }
}

#[must_use]
pub fn entry(output: &str) -> Option<Value> {
    crate::lock(channel()).entries.get(output).cloned()
}

pub fn publish(entries: &Map<String, Value>) {
    {
        let mut channel = crate::lock(channel());
        for (output, entry) in entries {
            channel.entries.insert(output.clone(), entry.clone());
        }
    }
    notify();
}

pub fn subscribed<'a>(mut outputs: impl Iterator<Item = &'a String>) -> bool {
    let channel = crate::lock(channel());
    let mut any = false;
    let all = outputs.all(|output| {
        any = true;
        channel.subscribers.contains_key(output)
    });
    any && all
}

#[cfg(test)]
mod tests;
