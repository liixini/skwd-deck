mod fullscreen;
mod niri_columns;
mod overview;
mod plasma;
pub(crate) mod processes;

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::{Notify, watch};
use tokio::time::Instant;

use crate::backend::events::EventPublisher;
use crate::composition::context::Ctx;

static REFRESH: Notify = Notify::const_new();
static STATUS: OnceLock<Mutex<Value>> = OnceLock::new();

pub(crate) fn refresh() {
    REFRESH.notify_one();
}
pub(crate) fn status() -> Value {
    skwd_wall_core::lock(STATUS.get_or_init(|| {
        Mutex::new(json!({"fullscreen_supported": false, "maximized_supported": false, "processes": [], "outputs": []}))
    }))
    .clone()
}

pub(crate) fn start(ctx: Ctx) {
    let (enabled, receive_enabled) = watch::channel(false);
    let (fullscreen, receive_fullscreen) = watch::channel(fullscreen::Snapshot::default());
    tokio::spawn(fullscreen::run(receive_enabled, fullscreen));
    let (overview_enabled, receive_overview_enabled) = watch::channel(false);
    let (overview, receive_overview) = watch::channel(None);
    tokio::spawn(overview::run(receive_overview_enabled, overview));
    let (columns_enabled, receive_columns_enabled) = watch::channel(false);
    let (columns, receive_columns) = watch::channel(niri_columns::Snapshot::default());
    tokio::spawn(niri_columns::run(receive_columns_enabled, columns));
    tokio::spawn(run(
        ctx,
        enabled,
        receive_fullscreen,
        overview_enabled,
        receive_overview,
        columns_enabled,
        receive_columns,
    ));
}

async fn run(
    ctx: Ctx,
    enabled: watch::Sender<bool>,
    mut fullscreen: watch::Receiver<fullscreen::Snapshot>,
    overview_enabled: watch::Sender<bool>,
    mut overview: watch::Receiver<Option<bool>>,
    columns_enabled: watch::Sender<bool>,
    mut columns: watch::Receiver<niri_columns::Snapshot>,
) {
    let mut held = HashSet::new();
    let mut last_desired = HashSet::new();
    let mut release_at = None;
    let mut process_at = Instant::now();
    let mut running = Vec::new();
    let mut previous_rules = String::new();
    let mut independent_playback = None;
    loop {
        let config = ctx.config.read().clone();
        let policy = config.playback();
        let overview_only =
            policy.overview_only() && config.renderer().wallpaper_layer() == "background";
        let observe_overview = overview_only || config.niri_overview_backdrop();
        overview_enabled.send_if_modified(|enabled| {
            let changed = *enabled != observe_overview;
            *enabled = observe_overview;
            changed
        });
        columns_enabled.send_if_modified(|enabled| {
            let changed = *enabled != policy.full_width_pause();
            *enabled = policy.full_width_pause();
            changed
        });
        let column_observation = columns.borrow().clone();
        let full_width_paused = policy.full_width_pause() && !column_observation.outputs.is_empty();
        let overview_open = *overview.borrow();
        let _ = tokio::task::spawn_blocking(move || {
            super::overview_backdrop::set_overview(overview_open);
        })
        .await;
        let overview_paused = overview::should_pause(overview_only, overview_open);
        let independent = policy.window_pause_enabled() && policy.fullscreen_scope() == "display";
        if independent_playback.replace(independent).is_some_and(|previous| previous != independent)
        {
            let state = ctx.state.clone();
            if let Ok(Err(error)) = tokio::task::spawn_blocking(move || {
                skwd_wall_core::apply::refresh_renderer_policy(&state)
            })
            .await
            {
                log::warn!("window playback policy: {error:#}");
            }
        }
        let rules =
            if policy.process_pause_enabled() { policy.pause_processes() } else { String::new() };
        enabled.send_if_modified(|value| {
            if *value == (policy.fullscreen_pause() || policy.maximized_pause()) {
                false
            } else {
                *value = policy.fullscreen_pause() || policy.maximized_pause();
                true
            }
        });
        if !rules.trim().is_empty() && (rules != previous_rules || Instant::now() >= process_at) {
            running = tokio::task::spawn_blocking(processes::running).await.unwrap_or_default();
            process_at = Instant::now() + Duration::from_secs(2);
        }
        previous_rules = rules.clone();
        let matched = processes::matches(&rules, &running);
        let observation = fullscreen.borrow().clone();
        let mut desired =
            if policy.fullscreen_pause() { observation.outputs.clone() } else { HashSet::new() };
        let maximized_paused =
            policy.maximized_pause() && !observation.maximized_outputs.is_empty();
        if policy.maximized_pause() {
            desired.extend(observation.maximized_outputs.iter().cloned());
        }
        if policy.full_width_pause() {
            desired.extend(column_observation.outputs.iter().cloned());
        }
        if !matched.is_empty() || (!desired.is_empty() && policy.fullscreen_scope() != "display") {
            desired.clear();
            desired.insert("*".to_string());
        }
        if desired != last_desired {
            held.extend(desired.iter().cloned());
            release_at = (held != desired).then(|| Instant::now() + policy.resume_delay());
            last_desired = desired.clone();
        }
        if release_at.is_some_and(|deadline| Instant::now() >= deadline) {
            held = desired;
            release_at = None;
        }
        let all = held.contains("*") || overview_paused;
        let outputs: HashSet<_> =
            held.iter().filter(|output| output.as_str() != "*").cloned().collect();
        let state = ctx.state.clone();
        let paused_outputs = outputs.clone();
        let observed_outputs = observation.observed_outputs.clone();
        let _ = tokio::task::spawn_blocking(move || {
            state.renderers().set_automatic_paused(all, paused_outputs);
            plasma::refresh_policy(&state, &observed_outputs);
        })
        .await;
        let mut names: Vec<_> = outputs.into_iter().collect();
        names.sort();
        let value = json!({"full_width_supported": column_observation.supported, "full_width_paused": full_width_paused, "fullscreen_supported": observation.supported, "maximized_supported": observation.maximized_supported, "window_state_backend": observation.backend, "maximized_paused": maximized_paused, "processes": matched,
            "outputs": names, "all_displays": all, "automatic_paused": !held.is_empty() || overview_paused, "resume_pending": release_at.is_some(), "overview_open": overview_open, "overview_paused": overview_paused});
        let changed = {
            let mut previous = skwd_wall_core::lock(STATUS.get_or_init(|| Mutex::new(Value::Null)));
            if *previous == value {
                false
            } else {
                *previous = value.clone();
                true
            }
        };
        if changed {
            ctx.events.publish(wall_proto::ev::PLAYBACK, value);
        }
        let process_deadline = (!rules.trim().is_empty()).then_some(process_at);
        let deadline = [process_deadline, release_at].into_iter().flatten().min();
        tokio::select! {
            () = REFRESH.notified() => {}
            changed = fullscreen.changed() => { if changed.is_err() { break; } }
            changed = columns.changed() => { if changed.is_err() { break; } }
            changed = overview.changed() => { if changed.is_err() { break; } }
            () = async { if let Some(deadline) = deadline { tokio::time::sleep_until(deadline).await; } else { std::future::pending::<()>().await; } } => {}
        }
    }
}
