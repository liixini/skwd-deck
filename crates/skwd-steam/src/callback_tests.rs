use super::pump;

#[tokio::test]
async fn pump_delivers_value() {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    sender.send(7_i32).unwrap();
    assert_eq!(pump(|| {}, &receiver, 20, "op").await, Ok(7));
}

#[tokio::test]
async fn pump_times_out() {
    let (_sender, receiver) = std::sync::mpsc::sync_channel::<i32>(1);
    let error = pump(|| {}, &receiver, 0, "op").await.unwrap_err();
    assert!(error.contains("op timed out after 0s"), "{error}");
}

#[tokio::test]
async fn pump_dropped_callback() {
    let (sender, receiver) = std::sync::mpsc::sync_channel::<i32>(1);
    drop(sender);
    let error = pump(|| {}, &receiver, 20, "op").await.unwrap_err();
    assert!(error.contains("op callback dropped"), "{error}");
}

#[tokio::test]
async fn pump_runs_callbacks() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let calls = AtomicUsize::new(0);
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let result = pump(
        || {
            if calls.fetch_add(1, Ordering::Relaxed) == 2 {
                let _ = sender.send(9_i32);
            }
        },
        &receiver,
        20,
        "op",
    )
    .await;
    assert_eq!(result, Ok(9));
    assert!(calls.load(Ordering::Relaxed) >= 3);
}
