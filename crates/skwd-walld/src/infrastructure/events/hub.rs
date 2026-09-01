use std::sync::{Arc, Mutex};

use serde_json::Value;
use tokio::sync::mpsc::Sender;
use tokio::sync::mpsc::error::TrySendError;
use wall_proto::Event;

use crate::backend::events::EventPublisher;
use crate::infrastructure::stats::Stats;

pub(crate) const SUBSCRIBER_CHANNEL_CAPACITY: usize = 4096;

struct Subscriber {
    id: u64,
    sender: Sender<String>,
}

pub(crate) struct EventHub {
    subscribers: Mutex<Vec<Subscriber>>,
    stats: Arc<Stats>,
}

impl EventHub {
    pub(crate) fn new(stats: Arc<Stats>) -> Self {
        Self { subscribers: Mutex::new(Vec::new()), stats }
    }

    pub(crate) fn subscribe(&self, id: u64, sender: Sender<String>) {
        skwd_wall_core::lock(&self.subscribers).push(Subscriber { id, sender });
    }

    pub(crate) fn unsubscribe(&self, id: u64) {
        skwd_wall_core::lock(&self.subscribers).retain(|subscriber| subscriber.id != id);
    }

    pub(crate) fn subscriber_count(&self) -> usize {
        skwd_wall_core::lock(&self.subscribers).len()
    }
}

impl EventPublisher for EventHub {
    fn publish(&self, event: &str, data: Value) {
        self.stats.event(event);
        let Ok(message) = serde_json::to_string(&Event { event: event.to_string(), data }) else {
            return;
        };
        skwd_wall_core::lock(&self.subscribers).retain(|subscriber| {
            match subscriber.sender.try_send(message.clone()) {
                Ok(()) => true,
                Err(TrySendError::Full(_)) => {
                    log::warn!("dropping subscriber: {SUBSCRIBER_CHANNEL_CAPACITY} events behind");
                    false
                }
                Err(TrySendError::Closed(_)) => false,
            }
        });
    }
}
