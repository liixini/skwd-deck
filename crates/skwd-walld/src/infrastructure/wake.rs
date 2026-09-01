use std::time::Duration;

use tokio::sync::Notify;
use tokio::sync::mpsc::UnboundedReceiver;

pub(crate) async fn wake_or_timeout(notify: &Notify, dur: Duration) -> bool {
    tokio::select! {
        biased;
        () = notify.notified() => false,
        () = tokio::time::sleep(dur) => true,
    }
}

pub(crate) fn drain_latest<T>(first: T, receiver: &mut UnboundedReceiver<T>) -> T {
    let mut latest = first;
    while let Ok(next) = receiver.try_recv() {
        latest = next;
    }
    latest
}
