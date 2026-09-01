#![cfg(test)]

use std::time::Duration;

use tokio::time::Instant;

use super::{ThemeJob, next_ready};

#[tokio::test(start_paused = true)]
async fn slot_latest_wins() {
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    sender
        .send(ThemeJob {
            path: "/w/a.png".into(),
            ready_at: Instant::now() + Duration::from_secs(1),
        })
        .unwrap();
    sender.send(ThemeJob { path: "/w/b.png".into(), ready_at: Instant::now() }).unwrap();
    let first = receiver.recv().await.unwrap();
    let job = next_ready(first, &mut receiver).await;
    assert_eq!(job.path, "/w/b.png");
    assert!(job.ready_at <= Instant::now());
    assert!(receiver.try_recv().is_err());
}

#[tokio::test(start_paused = true)]
async fn delayed_job_waits_until_ready() {
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let t0 = Instant::now();
    sender
        .send(ThemeJob { path: "/w/a.png".into(), ready_at: t0 + Duration::from_secs(300) })
        .unwrap();
    let first = receiver.recv().await.unwrap();
    let job = next_ready(first, &mut receiver).await;
    assert_eq!(job.path, "/w/a.png");
    assert!(Instant::now() >= t0 + Duration::from_secs(300));
}

#[tokio::test]
async fn delay_real_time() {
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let t0 = Instant::now();
    sender
        .send(ThemeJob { path: "/w/a.png".into(), ready_at: t0 + Duration::from_millis(30) })
        .unwrap();
    let first = receiver.recv().await.unwrap();
    let job = next_ready(first, &mut receiver).await;
    assert_eq!(job.path, "/w/a.png");
    assert!(t0.elapsed() >= Duration::from_millis(30));
}
