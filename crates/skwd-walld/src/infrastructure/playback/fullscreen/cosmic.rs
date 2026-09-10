use super::protocols::toplevel::{
    zcosmic_toplevel_handle_v1 as handle, zcosmic_toplevel_info_v1 as info,
};
use super::wlr::has_state;
use super::{Backend, Connection, Dispatch, Monitor, Proxy, QueueHandle, Window, wl_registry};
use wayland_client::{WEnum, event_created_child};
use wayland_protocols::ext::foreign_toplevel_list::v1::client::{
    ext_foreign_toplevel_handle_v1 as foreign, ext_foreign_toplevel_list_v1 as list,
};
use wayland_protocols::ext::workspace::v1::client::{
    ext_workspace_group_handle_v1 as group, ext_workspace_handle_v1 as workspace,
    ext_workspace_manager_v1 as manager,
};

#[derive(Default)]
pub(super) struct State {
    info: Option<info::ZcosmicToplevelInfoV1>,
    foreign: std::collections::HashMap<u32, foreign::ExtForeignToplevelHandleV1>,
    handles: std::collections::HashMap<u32, handle::ZcosmicToplevelHandleV1>,
    pending_workspaces: std::collections::HashSet<u32>,
}

impl State {
    fn attach(&mut self, qh: &QueueHandle<Monitor>) {
        if let Some(info) = &self.info {
            for (id, foreign) in &self.foreign {
                self.handles
                    .entry(*id)
                    .or_insert_with(|| info.get_cosmic_toplevel(foreign, qh, ()));
            }
        }
    }
}

pub(super) fn bind(
    state: &mut Monitor,
    registry: &wl_registry::WlRegistry,
    name: u32,
    interface: &str,
    version: u32,
    qh: &QueueHandle<Monitor>,
) {
    match interface {
        "zcosmic_toplevel_info_v1" if version >= 3 => {
            state.cosmic.info =
                Some(registry.bind::<info::ZcosmicToplevelInfoV1, _, _>(name, 3, qh, ()));
            state.backends.insert(Backend::Cosmic, name);
            state.cosmic.attach(qh);
        }
        "ext_foreign_toplevel_list_v1" => {
            registry.bind::<list::ExtForeignToplevelListV1, _, _>(name, 1, qh, ());
        }
        "ext_workspace_manager_v1" => {
            registry.bind::<manager::ExtWorkspaceManagerV1, _, _>(name, 1, qh, ());
        }
        _ => {}
    }
}

impl Dispatch<list::ExtForeignToplevelListV1, ()> for Monitor {
    fn event(
        state: &mut Self,
        _: &list::ExtForeignToplevelListV1,
        event: list::Event,
        (): &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            list::Event::Toplevel { toplevel } => {
                state.cosmic.foreign.insert(toplevel.id().protocol_id(), toplevel);
                state.cosmic.attach(qh);
            }
            list::Event::Finished => {
                state.backends.remove(&Backend::Cosmic);
                state.windows.retain(|_, window| window.backend != Backend::Cosmic);
                state.pending.retain(|_, window| window.backend != Backend::Cosmic);
            }
            _ => {}
        }
    }
    event_created_child!(Monitor, list::ExtForeignToplevelListV1, [0 => (foreign::ExtForeignToplevelHandleV1, ())]);
}

impl Dispatch<foreign::ExtForeignToplevelHandleV1, ()> for Monitor {
    fn event(
        state: &mut Self,
        object: &foreign::ExtForeignToplevelHandleV1,
        event: foreign::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, foreign::Event::Closed) {
            let id = object.id().protocol_id();
            state.cosmic.foreign.remove(&id);
            if let Some(handle) = state.cosmic.handles.remove(&id) {
                state.remove(handle.id().protocol_id());
                handle.destroy();
            }
            object.destroy();
        }
    }
}

impl Dispatch<info::ZcosmicToplevelInfoV1, ()> for Monitor {
    fn event(
        state: &mut Self,
        _: &info::ZcosmicToplevelInfoV1,
        event: info::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let info::Event::Done = event {
            for (id, window) in &state.pending {
                if window.backend == Backend::Cosmic {
                    state.windows.insert(*id, window.clone());
                }
            }
        }
    }
}

impl Dispatch<handle::ZcosmicToplevelHandleV1, ()> for Monitor {
    fn event(
        state: &mut Self,
        object: &handle::ZcosmicToplevelHandleV1,
        event: handle::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let window = state
            .pending
            .entry(object.id().protocol_id())
            .or_insert_with(|| Window { backend: Backend::Cosmic, ..Window::default() });
        match event {
            handle::Event::State { state: values } => {
                window.fullscreen = has_state(&values, handle::State::Fullscreen as u32);
                window.maximized = has_state(&values, handle::State::Maximized as u32);
                window.minimized = has_state(&values, handle::State::Minimized as u32);
                window.sticky = has_state(&values, handle::State::Sticky as u32);
            }
            handle::Event::OutputEnter { output } => {
                window.outputs.insert(output.id().protocol_id());
            }
            handle::Event::OutputLeave { output } => {
                window.outputs.remove(&output.id().protocol_id());
            }
            handle::Event::ExtWorkspaceEnter { workspace } => {
                window.workspaces.insert(workspace.id().protocol_id());
            }
            handle::Event::ExtWorkspaceLeave { workspace } => {
                window.workspaces.remove(&workspace.id().protocol_id());
            }
            _ => {}
        }
    }
}

impl Dispatch<manager::ExtWorkspaceManagerV1, ()> for Monitor {
    fn event(
        state: &mut Self,
        _: &manager::ExtWorkspaceManagerV1,
        event: manager::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            manager::Event::Done => {
                state.active_workspaces.clone_from(&state.cosmic.pending_workspaces);
            }
            manager::Event::Finished => {
                state.active_workspaces.clear();
                state.cosmic.pending_workspaces.clear();
            }
            _ => {}
        }
    }
    event_created_child!(Monitor, manager::ExtWorkspaceManagerV1, [0 => (group::ExtWorkspaceGroupHandleV1, ()), 1 => (workspace::ExtWorkspaceHandleV1, ())]);
}

impl Dispatch<group::ExtWorkspaceGroupHandleV1, ()> for Monitor {
    fn event(
        _: &mut Self,
        object: &group::ExtWorkspaceGroupHandleV1,
        event: group::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let group::Event::Removed = event {
            object.destroy();
        }
    }
}

impl Dispatch<workspace::ExtWorkspaceHandleV1, ()> for Monitor {
    fn event(
        state: &mut Self,
        object: &workspace::ExtWorkspaceHandleV1,
        event: workspace::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let id = object.id().protocol_id();
        match event {
            workspace::Event::State { state: WEnum::Value(values) } => {
                if values.contains(workspace::State::Active)
                    && !values.contains(workspace::State::Hidden)
                {
                    state.cosmic.pending_workspaces.insert(id);
                } else {
                    state.cosmic.pending_workspaces.remove(&id);
                }
            }
            workspace::Event::Removed => {
                state.cosmic.pending_workspaces.remove(&id);
                object.destroy();
            }
            _ => {}
        }
    }
}
