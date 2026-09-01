use std::time::{Duration, Instant};

pub async fn pump<T>(
    run_callbacks: impl Fn(),
    receiver: &std::sync::mpsc::Receiver<T>,
    seconds: u64,
    operation: &str,
) -> Result<T, String> {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    loop {
        run_callbacks();
        match receiver.try_recv() {
            Ok(value) => return Ok(value),
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                if Instant::now() > deadline {
                    return Err(format!("{operation} timed out after {seconds}s"));
                }
                tokio::time::sleep(Duration::from_millis(15)).await;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return Err(format!("{operation} callback dropped"));
            }
        }
    }
}

#[cfg(test)]
#[path = "callback_tests.rs"]
mod tests;
