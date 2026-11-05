use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use skwd_config::studio_time::{DAY_HOUR, NIGHT_HOUR, StudioMode, dark_at};
use tokio::sync::Notify;

use crate::infrastructure::wake::wake_or_timeout;

static WAKE: Notify = Notify::const_new();

struct Current {
    source: String,
    dark: bool,
    wait: Duration,
    applied: bool,
}

pub fn reload() {
    WAKE.notify_one();
}

pub fn start(ctx: crate::composition::context::Ctx) {
    tokio::spawn(clock_loop(ctx.state));
}

fn current(state: &skwd_wall_core::WallState) -> Option<Current> {
    state.reload_config();
    let source = state.theme().source()?;
    let config = skwd_wall_core::theme::profiles::configuration(state, &source);
    if config.theme().backend() != wall_proto::STUDIO_ENGINE
        || StudioMode::from_key(&config.theme().studio_mode()) != StudioMode::ByTime
    {
        return None;
    }
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |time| time.as_secs() as i64);
    let local = skwd_log::local_time(secs);
    let minute =
        (local.tm_hour * skwd_config::studio_time::MINUTES_PER_HOUR as i32 + local.tm_min) as u32;
    let dark = dark_at(minute);
    let target = if dark { DAY_HOUR } else { NIGHT_HOUR };
    let wait = Duration::from_secs(skwd_log::seconds_until_local_hour(secs, target));
    let applied = state.theme().applied_theme().is_some_and(|snapshot| {
        snapshot[skwd_wall_core::theme::profiles::APPLIED_SOURCE_KEY].as_str() == Some(&source)
            && snapshot[skwd_wall_core::theme::profiles::APPLIED_DARK_KEY].as_bool() == Some(dark)
    });
    Some(Current { source, dark, wait, applied })
}

async fn clock_loop(state: Arc<skwd_wall_core::WallState>) {
    loop {
        let check = Arc::clone(&state);
        let Ok(active) = tokio::task::spawn_blocking(move || current(&check)).await else {
            break;
        };
        if let Some(ref active) = active
            && !active.applied
        {
            crate::infrastructure::theme_worker::theme_apply_clock_async(
                &active.source,
                active.dark,
            );
        }
        if let Some(active) = active {
            wake_or_timeout(&WAKE, active.wait).await;
        } else {
            WAKE.notified().await;
        }
    }
}

#[cfg(test)]
#[path = "studio_clock_tests.rs"]
mod tests;
