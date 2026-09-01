use std::sync::{Arc, Mutex};

use serde_json::json;
use skwd_wall_core::{WallState, lock};

use crate::backend::events::EventPublisher;
use crate::backend::history::{ApplySource, HistoryRepository};
use crate::domain::history::HistoryEntry;
use crate::infrastructure::stats::Stats;

static LAST_APPLY_SOURCE: Mutex<Option<ApplySource>> = Mutex::new(None);

pub(crate) fn note_apply_source(source: ApplySource) {
    *lock(&LAST_APPLY_SOURCE) = Some(source);
}

pub(crate) fn last_apply_source() -> Option<ApplySource> {
    *lock(&LAST_APPLY_SOURCE)
}

pub(crate) fn record_history(
    state: &Arc<WallState>,
    history: &dyn HistoryRepository,
    output: &str,
    kind: &str,
    path: &str,
    we_id: &str,
    mute: bool,
    volume: u32,
    prior: Option<&HistoryEntry>,
) {
    let (enabled, depth) = {
        let config = state.config();
        (config.history_enabled(), config.history_depth())
    };
    if !enabled {
        return;
    }
    let entry = HistoryEntry::new(kind, path, we_id, mute, volume);
    let live = live_output_names(state);
    history.record(output, &entry, prior, depth, &live);
}

pub(crate) fn live_output_names(state: &Arc<WallState>) -> Vec<String> {
    let mut live = skwd_wall_core::outputs::names();
    if live.is_empty() {
        let cache = state.config().cache_dir();
        if let Some(map) = skwd_wall_core::audio::read_state(&cache).as_object() {
            live = map.keys().cloned().collect();
        }
    }
    live.retain(|output| output != "*");
    live
}

pub(crate) fn history_nav(
    state: &Arc<WallState>,
    application: &dyn skwd_wall_core::backend::wallpaper::WallpaperApplication,
    history: &dyn HistoryRepository,
    publisher: &dyn EventPublisher,
    stats: &Arc<Stats>,
    output: &str,
    forward: bool,
) -> serde_json::Value {
    state.reload_config();
    if !state.config().history_enabled() {
        return json!({"ok": false, "message": "wallpaper history is disabled"});
    }
    let live = live_output_names(state);
    let moved = history.navigate(output, forward, &live);
    if moved.is_empty() {
        let direction = if forward { "forward" } else { "back" };
        return json!({"ok": false, "message": format!("no {direction} history for {output}")});
    }
    for (target_output, entry) in &moved {
        if let Err(error) = crate::composition::apply::apply_core(
            state,
            application,
            history,
            publisher,
            stats,
            &entry.ty,
            &entry.path,
            &entry.we_id,
            entry.mute,
            entry.volume,
            ApplySource::Replay,
            target_output,
            true,
            false,
            None,
            None,
        ) {
            log::warn!("history replay failed on {target_output}: {error}");
        }
    }
    let outputs: Vec<String> = moved.into_iter().map(|(output, _)| output).collect();
    json!({"ok": true, "outputs": outputs})
}
