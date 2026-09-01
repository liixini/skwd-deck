use std::time::{Duration, Instant};

use steamworks::{Client, ItemState, PublishedFileId};

use crate::callback::pump;
use crate::event_output::emit;
use crate::policy;

pub async fn unsubscribe(client: &Client, id: &str) -> Result<(), String> {
    let raw: u64 = id.parse().map_err(|_| format!("invalid workshop id {id}"))?;
    let (sender, receiver) = std::sync::mpsc::sync_channel::<Result<(), String>>(1);
    client.ugc().unsubscribe_item(PublishedFileId(raw), move |result| {
        let _ = sender.send(result.map_err(|error| format!("{error:?}")));
    });
    pump(|| client.run_callbacks(), &receiver, 20, "unsubscribe")
        .await?
        .map_err(|error| format!("unsubscribe failed: {error}"))
}

fn install_complete(state: ItemState) -> bool {
    policy::install_complete(
        state.contains(ItemState::INSTALLED),
        state.contains(ItemState::DOWNLOADING),
        state.contains(ItemState::NEEDS_UPDATE),
    )
}

fn download_active(state: ItemState, total: u64) -> bool {
    policy::download_active(
        state.contains(ItemState::DOWNLOADING),
        state.contains(ItemState::DOWNLOAD_PENDING),
        total,
    )
}

pub async fn download(client: &Client, id: &str) -> Result<String, String> {
    let raw: u64 = id.parse().map_err(|_| format!("invalid workshop id {id}"))?;
    let item = PublishedFileId(raw);

    emit(id, "downloading", 0.0, "Subscribing...");
    let (sender, receiver) = std::sync::mpsc::sync_channel::<Result<(), String>>(1);
    client.ugc().subscribe_item(item, move |result| {
        let _ = sender.send(result.map_err(|error| format!("{error:?}")));
    });
    pump(|| client.run_callbacks(), &receiver, 20, "subscribe")
        .await?
        .map_err(|error| format!("subscribe failed: {error}"))?;
    if !client.ugc().download_item(item, true) {
        return Err("Steam rejected the download (not running, or item invalid)".into());
    }

    let deadline = Instant::now() + Duration::from_secs(60 * 30);
    let mut idle_since = None;
    let mut last = (0, 0);
    loop {
        client.run_callbacks();
        let state = client.ugc().item_state(item);
        let (current, total) = client.ugc().item_download_info(item).unwrap_or((0, 0));
        if (current, total) != last {
            last = (current, total);
            idle_since = None;
        }
        if install_complete(state) {
            return Ok(client
                .ugc()
                .item_install_info(item)
                .map(|info| info.folder)
                .unwrap_or_default());
        }

        let progress = policy::progress_fraction(current, total);
        emit(id, "downloading", progress, &format!("Downloading {}%", (progress * 100.0).round()));
        if download_active(state, total) {
            idle_since = None;
        } else {
            let now = Instant::now();
            let since = *idle_since.get_or_insert(now);
            if now.duration_since(since) > Duration::from_secs(15) {
                return Err(
                    "Steam did not start the download (is the WE-owning account logged in?)".into(),
                );
            }
        }
        if Instant::now() > deadline {
            return Err("download timed out after 30m".into());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}
