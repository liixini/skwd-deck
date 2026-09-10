mod cosmic;
mod outputs;
mod protocols;
mod wlr;

use std::collections::{HashMap, HashSet};
use std::os::fd::AsFd;
use std::time::Duration;

use tokio::io::unix::AsyncFd;
use tokio::sync::watch;
use wayland_client::protocol::{wl_output, wl_registry};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_manager_v1::ZxdgOutputManagerV1;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub supported: bool,
    pub maximized_supported: bool,
    pub backend: &'static str,
    pub outputs: HashSet<String>,
    pub maximized_outputs: HashSet<String>,
    pub observed_outputs: HashSet<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
enum Backend {
    #[default]
    Wlr,
    Cosmic,
}

#[derive(Clone, Default)]
#[allow(clippy::struct_excessive_bools, reason = "Independent compositor state flags can coexist")]
struct Window {
    backend: Backend,
    fullscreen: bool,
    maximized: bool,
    minimized: bool,
    sticky: bool,
    outputs: HashSet<u32>,
    workspaces: HashSet<u32>,
}

#[derive(Default)]
struct Monitor {
    registry_ready: bool,
    pending_wlr: Option<(u32, u32)>,
    backends: HashMap<Backend, u32>,
    outputs: HashMap<u32, String>,
    output_globals: HashMap<u32, wl_output::WlOutput>,
    output_manager: Option<ZxdgOutputManagerV1>,
    windows: HashMap<u32, Window>,
    pending: HashMap<u32, Window>,
    active_workspaces: HashSet<u32>,
    cosmic: cosmic::State,
}

impl Monitor {
    fn backend(&self) -> Option<Backend> {
        [Backend::Cosmic, Backend::Wlr]
            .into_iter()
            .find(|backend| self.backends.contains_key(backend))
    }

    fn visible(&self, window: &Window) -> bool {
        !window.minimized
            && Some(window.backend) == self.backend()
            && match window.backend {
                Backend::Cosmic => {
                    window.sticky
                        || window.workspaces.is_empty()
                        || window.workspaces.iter().any(|id| self.active_workspaces.contains(id))
                }
                Backend::Wlr => true,
            }
    }

    fn matching_outputs(&self, matches: impl Fn(&Window) -> bool) -> HashSet<String> {
        self.windows
            .values()
            .filter(|window| self.visible(window) && matches(window))
            .flat_map(|window| {
                self.outputs.iter().filter_map(move |(id, name)| {
                    let visible = window.outputs.contains(id);
                    visible.then(|| name.clone())
                })
            })
            .collect()
    }

    fn snapshot(&self) -> Snapshot {
        let supported = self.backend().is_some() && !self.outputs.is_empty();
        Snapshot {
            supported,
            maximized_supported: supported,
            backend: match self.backend() {
                Some(Backend::Wlr) => "wlr",
                Some(Backend::Cosmic) => "cosmic",
                None => "unavailable",
            },
            outputs: self.matching_outputs(|window| window.fullscreen),
            maximized_outputs: self.matching_outputs(|window| window.maximized),
            observed_outputs: self.outputs.values().cloned().collect(),
        }
    }

    fn commit(&mut self, id: u32) {
        if let Some(window) = self.pending.get(&id) {
            self.windows.insert(id, window.clone());
        }
    }

    fn remove(&mut self, id: u32) {
        self.pending.remove(&id);
        self.windows.remove(&id);
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for Monitor {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        (): &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global { name, interface, version } => {
                outputs::bind(state, registry, name, &interface, version, qh);
                wlr::bind(state, registry, name, &interface, version, qh);
                cosmic::bind(state, registry, name, &interface, version, qh);
            }
            wl_registry::Event::GlobalRemove { name } => {
                if state.pending_wlr.is_some_and(|(global, _)| global == name) {
                    state.pending_wlr = None;
                }
                state.backends.retain(|_, global| *global != name);
                if let Some(output) = state.output_globals.remove(&name) {
                    let id = output.id().protocol_id();
                    state.outputs.remove(&id);
                }
            }
            _ => {}
        }
    }
}

fn selected_snapshot(enabled: bool, wayland: &Snapshot, plasma: &Snapshot) -> Snapshot {
    let mut observation = if !enabled {
        Snapshot { backend: "unavailable", ..Snapshot::default() }
    } else if plasma.supported {
        plasma.clone()
    } else {
        wayland.clone()
    };
    observation.observed_outputs.extend(plasma.observed_outputs.iter().cloned());
    observation
}

pub(crate) async fn run(mut enabled: watch::Receiver<bool>, sender: watch::Sender<Snapshot>) {
    let (wayland_sender, mut wayland_state) = watch::channel(Snapshot::default());
    let (plasma_sender, mut plasma_state) = watch::channel(Snapshot::default());
    let mut tasks = tokio::task::JoinSet::new();
    tasks.spawn(wayland(enabled.clone(), wayland_sender));
    tasks.spawn(super::plasma::run(plasma_sender));
    loop {
        let observation =
            selected_snapshot(*enabled.borrow(), &wayland_state.borrow(), &plasma_state.borrow());
        sender.send_if_modified(|previous| {
            if *previous == observation {
                false
            } else {
                *previous = observation;
                true
            }
        });
        tokio::select! {
            changed = enabled.changed() => { if changed.is_err() { break; } }
            changed = wayland_state.changed() => { if changed.is_err() { break; } }
            changed = plasma_state.changed(), if plasma_state.has_changed().is_ok() => { let _ = changed; }
            () = sender.closed() => break,
        }
    }
}

async fn wayland(mut enabled: watch::Receiver<bool>, sender: watch::Sender<Snapshot>) {
    loop {
        if !*enabled.borrow() {
            if enabled.changed().await.is_err() {
                break;
            }
            continue;
        }
        if let Err(error) = connected(&mut enabled, &sender).await {
            log::debug!("window state detection unavailable: {error:#}");
        }
        sender.send_replace(Snapshot::default());
        tokio::select! {
            changed = enabled.changed() => { if changed.is_err() { break; } }
            () = tokio::time::sleep(Duration::from_secs(2)) => {}
        }
    }
}

async fn connected(
    enabled: &mut watch::Receiver<bool>,
    sender: &watch::Sender<Snapshot>,
) -> anyhow::Result<()> {
    let connection = Connection::connect_to_env()?;
    let mut queue = connection.new_event_queue::<Monitor>();
    let qh = queue.handle();
    let registry = connection.display().get_registry(&qh, ());
    connection.display().sync(&qh, registry);
    let fd = AsyncFd::new(connection.backend().poll_fd().as_fd().try_clone_to_owned()?)?;
    let mut monitor = Monitor::default();
    while *enabled.borrow() {
        queue.dispatch_pending(&mut monitor)?;
        sender.send_if_modified(|previous| {
            let next = monitor.snapshot();
            if *previous == next {
                false
            } else {
                *previous = next;
                true
            }
        });
        connection.flush()?;
        let Some(guard) = queue.prepare_read() else {
            continue;
        };
        tokio::select! {
            changed = enabled.changed() => { drop(guard); if changed.is_err() { break; } }
            ready = fd.readable() => {
                let mut ready = ready?;
                match guard.read() {
                    Ok(_) => {}
                    Err(wayland_client::backend::WaylandError::Io(error)) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(error) => return Err(error.into()),
                }
                ready.clear_ready();
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
