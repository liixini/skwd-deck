use std::sync::{Arc, OnceLock};

use skwd_wall_core::WallState;
use skwd_wall_core::infrastructure::plasma::LockScreenCurrent;
use tokio::sync::mpsc::UnboundedSender;

use crate::infrastructure::wake::drain_latest;

// The persisted record is global, so on mixed-output setups the last apply wins.
fn sync_current(state: &WallState) {
    let entry = super::persistence::current_entry();
    let poster = entry
        .as_ref()
        .and_then(|entry| {
            super::persistence::last_any_thumb().filter(|poster| {
                entry.ty == wall_proto::kind::STATIC || poster.as_str() != entry.path
            })
        })
        .unwrap_or_default();
    let current = entry.as_ref().map(|entry| LockScreenCurrent {
        kind: &entry.ty,
        path: &entry.path,
        we_id: &entry.we_id,
        poster: &poster,
    });
    match skwd_wall_core::infrastructure::plasma::sync_lock_screen(state, current.as_ref()) {
        Ok(true) => log::info!("synchronized KDE Plasma lock-screen wallpaper"),
        Ok(false) => {}
        Err(error) => log::warn!("could not synchronize KDE Plasma lock-screen wallpaper: {error}"),
    }
}

// Serialized off the apply path: KConfig writes and KScreenLocker reloads spawn helpers.
pub(crate) fn request_sync(state: &Arc<WallState>) {
    static SENDER: OnceLock<UnboundedSender<Arc<WallState>>> = OnceLock::new();
    let sender = SENDER.get_or_init(|| {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<Arc<WallState>>();
        tokio::spawn(async move {
            while let Some(first) = receiver.recv().await {
                let latest = drain_latest(first, &mut receiver);
                let _ = tokio::task::spawn_blocking(move || sync_current(&latest)).await;
            }
        });
        sender
    });
    let _ = sender.send(Arc::clone(state));
}

pub(crate) fn request_follow_sync(state: &Arc<WallState>) {
    if state.config().plasma_lock_screen_mode() == "follow" {
        request_sync(state);
    }
}
