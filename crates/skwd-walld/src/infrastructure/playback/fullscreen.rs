use std::collections::{HashMap, HashSet};
use std::os::fd::AsFd;

use tokio::io::unix::AsyncFd;
use tokio::sync::watch;
use wayland_client::protocol::{wl_output, wl_registry};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, event_created_child};
use wayland_protocols_wlr::foreign_toplevel::v1::client::{
    zwlr_foreign_toplevel_handle_v1 as handle, zwlr_foreign_toplevel_manager_v1 as manager,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub supported: bool,
    pub outputs: HashSet<String>,
    pub maximized_outputs: HashSet<String>,
}

#[derive(Default)]
struct Window {
    fullscreen: bool,
    maximized: bool,
    minimized: bool,
    outputs: HashSet<u32>,
}

#[derive(Default)]
struct Monitor {
    supported: bool,
    outputs: HashMap<u32, String>,
    windows: HashMap<u32, Window>,
}

impl Monitor {
    fn matching_outputs(&self, matches: impl Fn(&Window) -> bool) -> HashSet<String> {
        self.windows
            .values()
            .filter(|window| !window.minimized && matches(window))
            .flat_map(|window| window.outputs.iter())
            .filter_map(|id| self.outputs.get(id))
            .cloned()
            .collect()
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            supported: self.supported,
            outputs: self.matching_outputs(|window| window.fullscreen),
            maximized_outputs: self.matching_outputs(|window| window.maximized),
        }
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
        if let wl_registry::Event::Global { name, interface, version } = event {
            if interface == "zwlr_foreign_toplevel_manager_v1" && version >= 2 {
                registry.bind::<manager::ZwlrForeignToplevelManagerV1, _, _>(
                    name,
                    version.min(3),
                    qh,
                    (),
                );
                state.supported = true;
            } else if interface == "wl_output" && version >= 4 {
                registry.bind::<wl_output::WlOutput, _, _>(name, 4, qh, ());
            }
        }
    }
}

impl Dispatch<wl_output::WlOutput, ()> for Monitor {
    fn event(
        state: &mut Self,
        output: &wl_output::WlOutput,
        event: wl_output::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Name { name } = event {
            state.outputs.insert(output.id().protocol_id(), name);
        }
    }
}

impl Dispatch<manager::ZwlrForeignToplevelManagerV1, ()> for Monitor {
    fn event(
        state: &mut Self,
        _: &manager::ZwlrForeignToplevelManagerV1,
        event: manager::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let manager::Event::Finished = event {
            state.supported = false;
            state.windows.clear();
        }
    }
    event_created_child!(Monitor, manager::ZwlrForeignToplevelManagerV1, [0 => (handle::ZwlrForeignToplevelHandleV1, ())]);
}

impl Dispatch<handle::ZwlrForeignToplevelHandleV1, ()> for Monitor {
    fn event(
        state: &mut Self,
        object: &handle::ZwlrForeignToplevelHandleV1,
        event: handle::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let id = object.id().protocol_id();
        if matches!(event, handle::Event::Closed) {
            state.windows.remove(&id);
            object.destroy();
            return;
        }
        let window = state.windows.entry(id).or_default();
        match event {
            handle::Event::State { state } => {
                let values: Vec<_> = state
                    .chunks_exact(4)
                    .map(|bytes| u32::from_ne_bytes(bytes.try_into().unwrap()))
                    .collect();
                window.fullscreen = values.contains(&(handle::State::Fullscreen as u32));
                window.maximized = values.contains(&(handle::State::Maximized as u32));
                window.minimized = values.contains(&(handle::State::Minimized as u32));
            }
            handle::Event::OutputEnter { output } => {
                window.outputs.insert(output.id().protocol_id());
            }
            handle::Event::OutputLeave { output } => {
                window.outputs.remove(&output.id().protocol_id());
            }
            _ => {}
        }
    }
}

pub(crate) async fn run(mut enabled: watch::Receiver<bool>, sender: watch::Sender<Snapshot>) {
    loop {
        let active = *enabled.borrow();
        if active && let Err(error) = connected(&mut enabled, &sender).await {
            log::debug!("window state detection unavailable: {error:#}");
            sender.send_replace(Snapshot::default());
        }
        if enabled.changed().await.is_err() {
            break;
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
    connection.display().get_registry(&qh, ());
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
    sender.send_replace(Snapshot::default());
    Ok(())
}

#[cfg(test)]
mod tests;
