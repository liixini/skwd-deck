use std::ffi::OsString;
use std::os::fd::{AsFd, AsRawFd};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use wayland_client::protocol::{wl_callback, wl_registry};
use wayland_client::{Connection, Dispatch, QueueHandle};

const PROBE_TIMEOUT: Duration = Duration::from_millis(250);
const CACHE_TTL: Duration = Duration::from_secs(2);

struct CachedSession {
    endpoint: [Option<OsString>; 3],
    plasma: bool,
    checked: Instant,
}

pub(super) fn running_on_plasma() -> bool {
    static CACHE: Mutex<Option<CachedSession>> = Mutex::new(None);
    let endpoint = ["XDG_RUNTIME_DIR", "WAYLAND_DISPLAY", "WAYLAND_SOCKET"].map(std::env::var_os);
    let mut cache = crate::lock(&CACHE);
    if let Some(cached) = cache.as_ref()
        && cached.endpoint == endpoint
        && cached.checked.elapsed() < CACHE_TTL
    {
        return cached.plasma;
    }
    let result = Connection::connect_to_env().ok().as_ref().and_then(probe);
    if let Some(plasma) = result {
        *cache = Some(CachedSession { endpoint, plasma, checked: Instant::now() });
    }
    result.unwrap_or(false)
}

#[derive(Default)]
struct Registry {
    plasma: bool,
    done: bool,
}

fn probe(connection: &Connection) -> Option<bool> {
    let mut queue = connection.new_event_queue();
    let handle = queue.handle();
    let _registry = connection.display().get_registry(&handle, ());
    let _sync = connection.display().sync(&handle, ());
    let mut state = Registry::default();
    let deadline = Instant::now() + PROBE_TIMEOUT;
    loop {
        queue.dispatch_pending(&mut state).ok()?;
        if state.done {
            return Some(state.plasma);
        }
        let remaining = deadline.checked_duration_since(Instant::now())?;
        queue.flush().ok()?;
        let Some(read) = queue.prepare_read() else { continue };
        let mut poll =
            libc::pollfd { fd: queue.as_fd().as_raw_fd(), events: libc::POLLIN, revents: 0 };
        let ready = unsafe { libc::poll(&mut poll, 1, remaining.as_millis().max(1) as i32) };
        if ready < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
            continue;
        }
        if ready <= 0 {
            return None;
        }
        read.read().ok()?;
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for Registry {
    fn event(
        state: &mut Self,
        _: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { interface, .. } = event {
            state.plasma |= interface == "org_kde_plasma_shell";
        }
    }
}

impl Dispatch<wl_callback::WlCallback, ()> for Registry {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        _: wl_callback::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        state.done = true;
    }
}

#[cfg(test)]
mod tests;
