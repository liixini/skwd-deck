use std::sync::Arc;

use serde_json::json;
use tokio::sync::mpsc::Receiver;

use crate::backend::events::EventPublisher;
use crate::infrastructure::stats::Stats;

use super::EventHub;

fn drained(receiver: &mut Receiver<String>) -> usize {
    let mut count = 0;
    while receiver.try_recv().is_ok() {
        count += 1;
    }
    count
}

#[test]
fn publish_evicts_wedged_subscribers() {
    let hub = EventHub::new(Arc::new(Stats::new()));
    let (slow_sender, mut slow_receiver) = tokio::sync::mpsc::channel(1);
    let (ready_sender, mut ready_receiver) = tokio::sync::mpsc::channel(64);
    hub.subscribe(1, slow_sender);
    hub.subscribe(2, ready_sender);

    hub.publish("e.one", json!({}));
    hub.publish("e.two", json!({}));

    assert_eq!(hub.subscriber_count(), 1);
    assert_eq!(drained(&mut ready_receiver), 2);
    assert_eq!(drained(&mut slow_receiver), 1);
}

#[test]
fn unsubscribe_one() {
    let hub = EventHub::new(Arc::new(Stats::new()));
    let (first, _first_receiver) = tokio::sync::mpsc::channel(1);
    let (second, _second_receiver) = tokio::sync::mpsc::channel(1);
    hub.subscribe(10, first);
    hub.subscribe(11, second);

    hub.unsubscribe(10);

    assert_eq!(hub.subscriber_count(), 1);
}
