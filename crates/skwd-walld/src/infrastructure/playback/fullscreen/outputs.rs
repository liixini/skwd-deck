use super::{
    Connection, Dispatch, Monitor, Proxy, QueueHandle, ZxdgOutputManagerV1, wl_output, wl_registry,
};
use wayland_protocols::xdg::xdg_output::zv1::client::{zxdg_output_manager_v1, zxdg_output_v1};

pub(super) fn bind(
    state: &mut Monitor,
    registry: &wl_registry::WlRegistry,
    name: u32,
    interface: &str,
    version: u32,
    qh: &QueueHandle<Monitor>,
) {
    if interface == "wl_output" && version >= 2 {
        let output = registry.bind::<wl_output::WlOutput, _, _>(name, version.min(4), qh, ());
        if let Some(manager) = &state.output_manager {
            manager.get_xdg_output(&output, qh, output.id().protocol_id());
        }
        state.output_globals.insert(name, output);
    } else if interface == "zxdg_output_manager_v1" && version >= 2 {
        let manager = registry.bind::<ZxdgOutputManagerV1, _, _>(name, version.min(3), qh, ());
        for output in state.output_globals.values() {
            manager.get_xdg_output(output, qh, output.id().protocol_id());
        }
        state.output_manager = Some(manager);
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
        if let wl_output::Event::Name { name } = event
            && !name.is_empty()
        {
            state.outputs.insert(output.id().protocol_id(), name);
        }
    }
}

impl Dispatch<zxdg_output_v1::ZxdgOutputV1, u32> for Monitor {
    fn event(
        state: &mut Self,
        _: &zxdg_output_v1::ZxdgOutputV1,
        event: zxdg_output_v1::Event,
        id: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zxdg_output_v1::Event::Name { name } = event
            && !name.is_empty()
        {
            state.outputs.insert(*id, name);
        }
    }
}

impl Dispatch<ZxdgOutputManagerV1, ()> for Monitor {
    fn event(
        _: &mut Self,
        _: &ZxdgOutputManagerV1,
        _: zxdg_output_manager_v1::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
