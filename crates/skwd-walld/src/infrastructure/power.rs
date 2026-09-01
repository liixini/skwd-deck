use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use serde_json::json;
use skwd_config::PowerSourceState;
use tokio::io::Interest;
use tokio::io::unix::AsyncFd;
use tokio::sync::Notify;
use wall_proto::ev;

use crate::backend::events::EventPublisher;
use crate::composition::context::Ctx;

const POWER_POLL_INTERVAL: Duration = Duration::from_secs(10);
static REFRESH: Notify = Notify::const_new();
static STARTED: OnceLock<()> = OnceLock::new();

pub(crate) fn request_refresh() {
    REFRESH.notify_one();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PowerWake {
    Refresh,
    Changed,
    Retry,
    Ignore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
struct Observation {
    on_battery: bool,
    source_changed: bool,
    reconcile_needed: bool,
    retrying: bool,
}

#[derive(Debug)]
struct PolicyTracker {
    observed: Option<bool>,
    applied: Option<bool>,
    retry_pending: bool,
}

impl PolicyTracker {
    fn new(initial: PowerSourceState) -> Self {
        let known = initial.on_battery();
        Self { observed: known, applied: known, retry_pending: false }
    }

    fn observe(&mut self, source: PowerSourceState) -> Option<Observation> {
        let on_battery = source.on_battery()?;
        let source_changed = self.observed.replace(on_battery) != Some(on_battery);
        Some(Observation {
            on_battery,
            source_changed,
            reconcile_needed: self.retry_pending || self.applied != Some(on_battery),
            retrying: self.retry_pending,
        })
    }

    fn mark_current(&mut self, on_battery: bool) {
        self.applied = Some(on_battery);
        self.retry_pending = false;
    }

    fn mark_failed(&mut self) {
        self.retry_pending = true;
    }
}

fn source_label(source: PowerSourceState) -> &'static str {
    match source {
        PowerSourceState::OnBattery => "battery",
        PowerSourceState::ExternalPower => "external power",
        PowerSourceState::NoSystemBattery => "no system battery",
        PowerSourceState::Unknown => "unknown",
    }
}

pub(crate) fn start(ctx: Ctx) {
    let Ctx { state, events, .. } = ctx;
    let initial = skwd_config::power_source_state();
    skwd_config::set_power_source_snapshot(initial);
    if STARTED.set(()).is_err() {
        log::debug!("power policy watcher is already active");
        return;
    }
    tokio::spawn(watch(state, events, initial));
}

async fn watch(
    state: Arc<skwd_wall_core::WallState>,
    events: Arc<crate::infrastructure::events::EventHub>,
    initial: PowerSourceState,
) {
    let mut last_source = initial;
    let mut last_percent = skwd_config::battery_percent();
    let mut tracker = PolicyTracker::new(initial);
    let mut power_events = match watch_socket() {
        Ok(socket) => Some(socket),
        Err(error) => {
            log::warn!(
                "power event socket unavailable; falling back to {}s polling: {error}",
                POWER_POLL_INTERVAL.as_secs()
            );
            None
        }
    };
    log::debug!("power policy watcher active: initial source is {}", source_label(initial));
    loop {
        let retry_timer = tracker.retry_pending || power_events.is_none();
        let wake = match next_wake(power_events.as_ref(), retry_timer).await {
            Ok(wake) => wake,
            Err(error) => {
                log::warn!(
                    "power event socket stopped; falling back to {}s polling: {error}",
                    POWER_POLL_INTERVAL.as_secs()
                );
                power_events = None;
                PowerWake::Retry
            }
        };
        let forced = wake == PowerWake::Refresh;
        match wake {
            PowerWake::Ignore => continue,
            PowerWake::Retry if power_events.is_some() && !tracker.retry_pending => continue,
            PowerWake::Refresh | PowerWake::Changed | PowerWake::Retry => {}
        }
        let source = skwd_config::power_source_state();
        let percent = skwd_config::battery_percent();
        skwd_config::set_power_source_snapshot(source);
        let source_changed = source != last_source;
        if source_changed {
            log::info!(
                "power source observation changed from {} to {}",
                source_label(last_source),
                source_label(source)
            );
            last_source = source;
        }
        if source_changed || percent != last_percent {
            crate::composition::runtime::schedule::reload();
            last_percent = percent;
        }
        let observation = tracker.observe(source);
        if observation.is_some_and(|observation| observation.source_changed) {
            events.publish(
                ev::POWER_CHANGED,
                json!({
                    "on_battery": observation.expect("known observation").on_battery,
                    "source": source_label(source),
                }),
            );
        }
        if observation.is_none() && !forced {
            log::debug!("power source is unknown; retaining the last applied renderer policy");
            continue;
        }
        let on_battery = observation
            .map(|observation| observation.on_battery)
            .or(tracker.applied)
            .or(tracker.observed)
            .unwrap_or(false);
        let retrying =
            observation.is_some_and(|observation| observation.retrying) || tracker.retry_pending;
        let reconcile_needed =
            forced || observation.is_some_and(|observation| observation.reconcile_needed);
        if !reconcile_needed {
            continue;
        }
        // A failed refresh stays authoritative even once the renderer recorded the signature.
        if !retrying && skwd_wall_core::apply::active_renderer_policy_matches(&state) {
            log::debug!(
                "power/config change did not alter the active renderer policy; keeping the live renderer"
            );
            tracker.mark_current(on_battery);
            continue;
        }
        log::info!(
            "{} renderer power policy for {}",
            if retrying { "retrying" } else { "refreshing" },
            if on_battery { "battery" } else { "external power" }
        );
        let refresh_state = Arc::clone(&state);
        let refreshed = tokio::task::spawn_blocking(move || {
            skwd_wall_core::apply::refresh_renderer_policy(&refresh_state)
        })
        .await;
        match refreshed {
            Ok(Ok(())) => tracker.mark_current(on_battery),
            Ok(Err(error)) => {
                tracker.mark_failed();
                log::warn!(
                    "power policy renderer refresh failed; retrying after {}s: {error:#}",
                    POWER_POLL_INTERVAL.as_secs()
                );
            }
            Err(error) => {
                tracker.mark_failed();
                log::warn!(
                    "power policy renderer refresh failed; retrying after {}s: {error}",
                    POWER_POLL_INTERVAL.as_secs()
                );
            }
        }
    }
}

fn power_event_socket() -> io::Result<OwnedFd> {
    let raw = unsafe {
        libc::socket(
            libc::AF_NETLINK,
            libc::SOCK_DGRAM | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
            libc::NETLINK_KOBJECT_UEVENT,
        )
    };
    if raw < 0 {
        return Err(io::Error::last_os_error());
    }
    let socket = unsafe { OwnedFd::from_raw_fd(raw) };
    let mut address: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
    address.nl_family = libc::AF_NETLINK as libc::sa_family_t;
    address.nl_groups = 1;
    let result = unsafe {
        libc::bind(
            socket.as_raw_fd(),
            std::ptr::from_ref(&address).cast::<libc::sockaddr>(),
            std::mem::size_of::<libc::sockaddr_nl>() as libc::socklen_t,
        )
    };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(socket)
}

fn watch_socket() -> io::Result<AsyncFd<OwnedFd>> {
    AsyncFd::with_interest(power_event_socket()?, Interest::READABLE)
}

async fn next_wake(
    power_events: Option<&AsyncFd<OwnedFd>>,
    retry_timer: bool,
) -> io::Result<PowerWake> {
    tokio::select! {
        () = REFRESH.notified() => Ok(PowerWake::Refresh),
        wake = power_event_wake(power_events) => wake,
        () = retry_sleep(retry_timer) => Ok(PowerWake::Retry),
    }
}

async fn retry_sleep(armed: bool) {
    if armed {
        tokio::time::sleep(POWER_POLL_INTERVAL).await;
    } else {
        std::future::pending::<()>().await;
    }
}

async fn power_event_wake(socket: Option<&AsyncFd<OwnedFd>>) -> io::Result<PowerWake> {
    let Some(socket) = socket else {
        return std::future::pending().await;
    };
    let mut guard = socket.readable().await?;
    match guard.try_io(read_power_event) {
        Ok(Ok(true)) => Ok(PowerWake::Changed),
        Ok(Ok(false)) | Err(_) => Ok(PowerWake::Ignore),
        Ok(Err(error)) if error.kind() == io::ErrorKind::Interrupted => Ok(PowerWake::Ignore),
        Ok(Err(error)) => Err(error),
    }
}

fn read_power_event(socket: &AsyncFd<OwnedFd>) -> io::Result<bool> {
    let mut buffer = [0_u8; 8_192];
    let read = unsafe {
        libc::recv(socket.as_raw_fd(), buffer.as_mut_ptr().cast(), buffer.len(), libc::MSG_DONTWAIT)
    };
    if read < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(is_power_event(&buffer[..read as usize]))
}

fn is_power_event(message: &[u8]) -> bool {
    message.split(|byte| *byte == 0).any(|field| field == b"SUBSYSTEM=power_supply")
}

#[cfg(test)]
mod tests;
