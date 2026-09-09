use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::watch;

pub(super) fn should_pause(enabled: bool, open: Option<bool>) -> bool {
    enabled && open == Some(false)
}

fn parse_open(line: &str) -> Option<bool> {
    serde_json::from_str::<Value>(line)
        .ok()?
        .get("OverviewOpenedOrClosed")?
        .get("is_open")?
        .as_bool()
}

pub(super) async fn run(mut enabled: watch::Receiver<bool>, sender: watch::Sender<Option<bool>>) {
    loop {
        let active = *enabled.borrow();
        let socket = std::env::var_os("NIRI_SOCKET");
        if active && let Some(socket) = socket {
            tokio::select! {
                result = connected(std::path::Path::new(&socket), &sender) => {
                    if let Err(error) = result { log::debug!("Niri overview detection unavailable: {error}"); }
                }
                changed = enabled.changed() => { if changed.is_err() { return; } }
            }
            sender.send_replace(None);
            if *enabled.borrow() {
                tokio::select! {
                    () = tokio::time::sleep(std::time::Duration::from_secs(2)) => {}
                    changed = enabled.changed() => { if changed.is_err() { return; } }
                }
            }
        } else if enabled.changed().await.is_err() {
            return;
        }
    }
}

async fn connected(
    socket: &std::path::Path,
    sender: &watch::Sender<Option<bool>>,
) -> std::io::Result<()> {
    let mut stream = UnixStream::connect(socket).await?;
    stream.write_all(b"\"EventStream\"\n").await?;
    let mut lines = BufReader::new(stream).lines();
    while let Some(line) = lines.next_line().await? {
        if let Some(open) = parse_open(&line) {
            sender.send_if_modified(|previous| {
                let changed = *previous != Some(open);
                *previous = Some(open);
                changed
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
