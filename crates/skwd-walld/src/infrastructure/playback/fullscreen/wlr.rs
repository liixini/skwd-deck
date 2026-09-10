use super::{Backend, Connection, Dispatch, Monitor, Proxy, QueueHandle, wl_registry};
use wayland_client::event_created_child;
use wayland_client::protocol::wl_callback;
use wayland_protocols_wlr::foreign_toplevel::v1::client::{
    zwlr_foreign_toplevel_handle_v1 as handle, zwlr_foreign_toplevel_manager_v1 as manager,
};

pub(super) fn bind(
    state: &mut Monitor,
    registry: &wl_registry::WlRegistry,
    name: u32,
    interface: &str,
    version: u32,
    qh: &QueueHandle<Monitor>,
) {
    if interface == "zwlr_foreign_toplevel_manager_v1" && version >= 2 {
        if !state.registry_ready {
            state.pending_wlr = Some((name, version));
            return;
        }
        registry.bind::<manager::ZwlrForeignToplevelManagerV1, _, _>(name, version.min(3), qh, ());
        state.backends.insert(Backend::Wlr, name);
    }
}

impl Dispatch<wl_callback::WlCallback, wl_registry::WlRegistry> for Monitor {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        _: wl_callback::Event,
        registry: &wl_registry::WlRegistry,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        state.registry_ready = true;
        if let Some((name, version)) = state.pending_wlr.take() {
            bind(state, registry, name, "zwlr_foreign_toplevel_manager_v1", version, qh);
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
            state.backends.remove(&Backend::Wlr);
            state.windows.retain(|_, window| window.backend != Backend::Wlr);
            state.pending.retain(|_, window| window.backend != Backend::Wlr);
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
        match event {
            handle::Event::Closed => {
                state.remove(id);
                object.destroy();
            }
            handle::Event::Done => state.commit(id),
            handle::Event::State { state: values } => {
                let window = state.pending.entry(id).or_default();
                window.fullscreen = has_state(&values, handle::State::Fullscreen as u32);
                window.maximized = has_state(&values, handle::State::Maximized as u32);
                window.minimized = has_state(&values, handle::State::Minimized as u32);
            }
            handle::Event::OutputEnter { output } => {
                state.pending.entry(id).or_default().outputs.insert(output.id().protocol_id());
            }
            handle::Event::OutputLeave { output } => {
                state.pending.entry(id).or_default().outputs.remove(&output.id().protocol_id());
            }
            _ => {}
        }
    }
}

pub(super) fn has_state(values: &[u8], expected: u32) -> bool {
    values.chunks_exact(4).any(|bytes| u32::from_ne_bytes(bytes.try_into().unwrap()) == expected)
}
