use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::watch;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Snapshot {
    pub supported: bool,
    pub outputs: HashSet<String>,
    pub focused_outputs: HashSet<String>,
}

#[derive(Deserialize)]
struct Workspace {
    id: u64,
    output: Option<String>,
    is_active: bool,
    #[serde(default)]
    is_focused: bool,
    #[serde(default)]
    active_window_id: Option<u64>,
}

#[derive(Deserialize)]
struct Window {
    id: u64,
    workspace_id: Option<u64>,
    #[serde(default)]
    is_focused: bool,
    is_floating: bool,
    layout: Geometry,
}

#[derive(Deserialize)]
struct Geometry {
    pos_in_scrolling_layout: Option<(u64, u64)>,
    tile_size: (f64, f64),
    tile_pos_in_workspace_view: Option<(f64, f64)>,
}

#[derive(Default)]
struct Monitor {
    workspaces: HashMap<u64, Workspace>,
    windows: HashMap<u64, Window>,
    outputs: HashMap<String, (f64, f64)>,
    focused_window_id: Option<u64>,
    received_workspaces: bool,
    received_windows: bool,
}

impl Monitor {
    fn focused_workspace_window(&self) -> Option<u64> {
        self.workspaces
            .values()
            .find(|workspace| workspace.is_focused)
            .and_then(|workspace| workspace.active_window_id)
    }

    fn event(&mut self, value: &Value) -> Option<bool> {
        if let Some(event) = value.get("WorkspacesChanged") {
            let workspaces: Vec<Workspace> =
                serde_json::from_value(event.get("workspaces")?.clone()).ok()?;
            self.workspaces = workspaces.into_iter().map(|w| (w.id, w)).collect();
            self.focused_window_id = self.focused_workspace_window();
            self.received_workspaces = true;
            return Some(true);
        }
        if let Some(event) = value.get("WorkspaceActivated") {
            let id = event.get("id")?.as_u64()?;
            let focused = event.get("focused")?.as_bool()?;
            let output = self.workspaces.get(&id)?.output.clone();
            for workspace in self.workspaces.values_mut() {
                if workspace.output == output {
                    workspace.is_active = workspace.id == id;
                }
                if focused {
                    workspace.is_focused = workspace.id == id;
                }
            }
            if focused {
                self.focused_window_id = self.focused_workspace_window();
            }
            return Some(false);
        }
        if let Some(event) = value.get("WorkspaceActiveWindowChanged") {
            let workspace = self.workspaces.get_mut(&event.get("workspace_id")?.as_u64()?)?;
            workspace.active_window_id = event.get("active_window_id")?.as_u64();
            if workspace.is_focused {
                self.focused_window_id = workspace.active_window_id;
            }
            return Some(false);
        }
        if let Some(event) = value.get("WindowsChanged") {
            let windows: Vec<Window> =
                serde_json::from_value(event.get("windows")?.clone()).ok()?;
            self.focused_window_id = windows
                .iter()
                .find(|window| window.is_focused)
                .map(|window| window.id)
                .or_else(|| self.focused_workspace_window());
            self.windows = windows.into_iter().map(|w| (w.id, w)).collect();
            self.received_windows = true;
            return Some(true);
        }
        if let Some(event) = value.get("WindowOpenedOrChanged") {
            let window: Window = serde_json::from_value(event.get("window")?.clone()).ok()?;
            if window.is_focused {
                self.focused_window_id = Some(window.id);
            }
            self.windows.insert(window.id, window);
            return Some(true);
        }
        if let Some(event) = value.get("WindowClosed") {
            let id = event.get("id")?.as_u64()?;
            self.windows.remove(&id);
            if self.focused_window_id == Some(id) {
                self.focused_window_id = None;
            }
            return Some(false);
        }
        if let Some(event) = value.get("WindowFocusChanged") {
            self.focused_window_id = event.get("id").and_then(Value::as_u64);
            return Some(false);
        }
        if let Some(event) = value.get("WindowLayoutsChanged") {
            let changes: Vec<(u64, Geometry)> =
                serde_json::from_value(event.get("changes")?.clone()).ok()?;
            for (id, layout) in changes {
                if let Some(window) = self.windows.get_mut(&id) {
                    window.layout = layout;
                }
            }
            return Some(true);
        }
        None
    }

    fn set_outputs(&mut self, value: &Value) -> Option<()> {
        let outputs = value.get("Ok")?.get("Outputs")?.as_object()?;
        self.outputs = outputs
            .iter()
            .filter_map(|(name, output)| {
                let logical = output.get("logical")?;
                let width = logical.get("width")?.as_f64()?;
                let height = logical.get("height")?.as_f64()?;
                (width > 0.0 && height > 0.0).then(|| (name.clone(), (width, height)))
            })
            .collect();
        Some(())
    }

    fn snapshot(&self) -> Snapshot {
        let outputs = self
            .windows
            .values()
            .filter_map(|window| {
                if window.is_floating || window.layout.pos_in_scrolling_layout.is_none() {
                    return None;
                }
                let workspace = self.workspaces.get(&window.workspace_id?)?;
                if !workspace.is_active {
                    return None;
                }
                let output = workspace.output.as_ref()?;
                let (width, height) = *self.outputs.get(output)?;
                let (x, y) = if let Some(position) = window.layout.tile_pos_in_workspace_view {
                    position
                } else {
                    let active = self.windows.get(&workspace.active_window_id?)?;
                    if active.layout.pos_in_scrolling_layout?.0
                        != window.layout.pos_in_scrolling_layout?.0
                    {
                        return None;
                    }
                    (0.0, 0.0)
                };
                let (tile_width, tile_height) = window.layout.tile_size;
                let visible_width = (x + tile_width).min(width) - x.max(0.0);
                let visible_height = (y + tile_height).min(height) - y.max(0.0);
                (visible_width >= width * 0.9 && visible_height > 0.0).then(|| output.clone())
            })
            .collect();
        let focused_outputs = self
            .focused_window_id
            .and_then(|id| self.windows.get(&id))
            .and_then(|window| self.workspaces.get(&window.workspace_id?))
            .and_then(|workspace| workspace.output.as_ref())
            .filter(|output| self.outputs.contains_key(*output))
            .cloned()
            .into_iter()
            .collect();
        Snapshot {
            supported: self.received_windows
                && self.received_workspaces
                && !self.outputs.is_empty(),
            outputs,
            focused_outputs,
        }
    }
}

pub(super) fn available() -> bool {
    std::env::var_os("NIRI_SOCKET").is_some()
}

pub(super) async fn run(mut enabled: watch::Receiver<bool>, sender: watch::Sender<Snapshot>) {
    loop {
        let active = *enabled.borrow();
        let socket = std::env::var_os("NIRI_SOCKET");
        if active && let Some(socket) = socket {
            tokio::select! {
                result = connected(Path::new(&socket), &sender) => {
                    if let Err(error) = result { log::debug!("Niri column detection unavailable: {error}"); }
                }
                changed = enabled.changed() => { if changed.is_err() { return; } }
            }
            sender.send_replace(Snapshot::default());
            if *enabled.borrow() {
                tokio::select! {
                    () = tokio::time::sleep(Duration::from_secs(2)) => {}
                    changed = enabled.changed() => { if changed.is_err() { return; } }
                }
            }
        } else if enabled.changed().await.is_err() {
            return;
        }
    }
}

async fn connected(socket: &Path, sender: &watch::Sender<Snapshot>) -> anyhow::Result<()> {
    let mut stream = UnixStream::connect(socket).await?;
    stream.write_all(b"\"EventStream\"\n").await?;
    let mut lines = BufReader::new(stream).lines();
    let mut monitor = Monitor::default();
    while let Some(line) = lines.next_line().await? {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(refresh_outputs) = monitor.event(&value) else {
            continue;
        };
        if refresh_outputs {
            let reply = tokio::time::timeout(Duration::from_secs(2), async {
                let mut stream = UnixStream::connect(socket).await?;
                stream.write_all(b"\"Outputs\"\n").await?;
                let mut line = String::new();
                BufReader::new(stream).read_line(&mut line).await?;
                Ok::<_, std::io::Error>(line)
            })
            .await??;
            let value = serde_json::from_str(&reply)?;
            anyhow::ensure!(monitor.set_outputs(&value).is_some(), "invalid Niri output response");
        }
        sender.send_if_modified(|previous| {
            let next = monitor.snapshot();
            if *previous == next {
                false
            } else {
                *previous = next;
                true
            }
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
