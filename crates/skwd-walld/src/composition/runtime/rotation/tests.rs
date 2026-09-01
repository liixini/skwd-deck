#![cfg(test)]

use super::*;
use crate::infrastructure::wake::wake_or_timeout;

#[tokio::test]
async fn preset_wake_consumed() {
    wake();
    assert!(!wake_or_timeout(&WAKE, Duration::from_secs(3600)).await);
    let idle = Notify::new();
    assert!(wake_or_timeout(&idle, Duration::from_millis(15)).await);
}
