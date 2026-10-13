use std::sync::{Arc, OnceLock};
use std::time::Duration;

use serde_json::json;
use tokio::sync::mpsc;
use tokio::time::Instant;
use wall_proto::ev;

use crate::backend::events::EventPublisher;
use crate::infrastructure::wake::drain_latest;

#[derive(Clone)]
struct ThemeJob {
    path: String,
    ready_at: Instant,
    weather_generation: Option<u64>,
}

impl ThemeJob {
    fn current(&self, state: &skwd_wall_core::WallState) -> bool {
        self.weather_generation.is_none_or(|generation| {
            let config = state.config();
            crate::composition::runtime::outdoors::active(&config)
                && config.outdoors_retheme()
                && state.theme().source_generation() == generation
                && state.theme().source().as_deref() == Some(self.path.as_str())
        })
    }
}

static THEME_TX: OnceLock<mpsc::UnboundedSender<ThemeJob>> = OnceLock::new();

pub(crate) fn theme_apply_async(path: &str) {
    theme_apply_after_async(path, Duration::ZERO, None);
}

pub(crate) fn theme_apply_after_async(
    path: &str,
    delay: Duration,
    weather_generation: Option<u64>,
) {
    match THEME_TX.get() {
        Some(sender) => {
            let job = ThemeJob {
                path: path.to_string(),
                ready_at: Instant::now() + delay,
                weather_generation,
            };
            let _ = sender.send(job);
        }
        None => log::warn!("theme worker not started; dropping theme apply for {path}"),
    }
}

pub(crate) fn start_theme_worker(ctx: crate::composition::context::Ctx) {
    let crate::composition::context::Ctx { state, events, .. } = ctx;
    let (sender, receiver) = mpsc::unbounded_channel();
    let _ = THEME_TX.set(sender);
    tokio::spawn(worker(receiver, state, events));
}

async fn next_ready(first: ThemeJob, receiver: &mut mpsc::UnboundedReceiver<ThemeJob>) -> ThemeJob {
    let mut job = drain_latest(first, receiver);
    while job.ready_at > Instant::now() {
        tokio::select! {
            () = tokio::time::sleep_until(job.ready_at) => {}
            newer = receiver.recv() => {
                match newer {
                    Some(newer) => job = drain_latest(newer, receiver),
                    None => tokio::time::sleep_until(job.ready_at).await,
                }
            }
        }
    }
    job
}

async fn worker(
    mut receiver: mpsc::UnboundedReceiver<ThemeJob>,
    state: Arc<skwd_wall_core::WallState>,
    events: Arc<crate::infrastructure::events::EventHub>,
) {
    while let Some(first) = receiver.recv().await {
        let job = next_ready(first, &mut receiver).await;
        let apply_state = Arc::clone(&state);
        let apply_events = Arc::clone(&events);
        let _ = tokio::task::spawn_blocking(move || {
            apply_theme(&apply_state, apply_events.as_ref(), &job);
        })
        .await;
    }
}

fn apply_theme(state: &skwd_wall_core::WallState, events: &dyn EventPublisher, job: &ThemeJob) {
    let path = &job.path;
    state.reload_config();
    if !job.current(state) {
        return;
    }
    state.theme().bump_shell_preview();
    for sink in &skwd_wall_core::theme_sink::SINKS {
        (sink.forget)(state);
    }
    let _serial = state.theme().lock_shell_preview();
    state.reload_config();
    if !job.current(state) {
        return;
    }
    let mut cfg = skwd_wall_core::theme::profiles::configuration(state, path);
    if job.weather_generation.is_some() {
        cfg = cfg.with_override(
            skwd_config::keys::theme::POLICY,
            json!(skwd_config::keys::theme::POLICY_WALLPAPER),
        );
    }
    let requested = cfg.theme().backend();
    let ok = skwd_wall_core::theme::profiles::apply(state, path, &cfg);
    if ok && let Err(error) = skwd_wall_core::theme::profiles::remember_applied(state, path, &cfg) {
        log::warn!("current palette snapshot failed: {error:#}");
    }
    let effective = skwd_wall_core::theme::effective_backend(&cfg);
    events.publish(
        ev::THEME_DONE,
        json!({ "source": path, "ok": ok, "backend": effective, "requested": requested }),
    );
}

mod tests;
