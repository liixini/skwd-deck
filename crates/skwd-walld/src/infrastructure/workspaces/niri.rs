use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::Value;
use skwd_wall_core::WallState;

use super::model::WorkspaceInfo;
use super::policy::{mark_active, parse_rules, refresh_pending};
use super::runtime::{Engine, RECONNECT_DELAY, rt_lock};

pub(super) fn parse_topology(value: &Value) -> Option<HashMap<u64, WorkspaceInfo>> {
    let workspaces = value.get("WorkspacesChanged")?.get("workspaces")?.as_array()?;
    let mut topology = HashMap::new();
    for workspace in workspaces {
        let Some(id) = workspace.get("id").and_then(Value::as_u64) else {
            continue;
        };
        let output = workspace.get("output").and_then(Value::as_str).unwrap_or("").to_string();
        let idx = workspace.get("idx").and_then(Value::as_u64).unwrap_or(0);
        let name = workspace
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
            .map(str::to_string);
        let active = workspace.get("is_active").and_then(Value::as_bool).unwrap_or(false);
        topology.insert(id, WorkspaceInfo { output, idx, name, active });
    }
    Some(topology)
}

pub(super) fn parse_activated(value: &Value) -> Option<u64> {
    value.get("WorkspaceActivated")?.get("id")?.as_u64()
}

fn handle_line(engine: &Engine, state: &WallState, line: &str) {
    if !line.contains("\"WorkspacesChanged\"") && !line.contains("\"WorkspaceActivated\"") {
        return;
    }
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return;
    };
    state.reload_config();
    let (enabled, rules, debounce) = {
        let config = state.config();
        (
            config.workspace_enabled(),
            parse_rules(&config.workspace_wallpapers()),
            config.workspace_debounce_ms(),
        )
    };
    let mut runtime = rt_lock(engine);
    if let Some(topology) = parse_topology(&value) {
        runtime.topo = topology;
        runtime.dirs.clear();
        log::info!("workspace: topology snapshot ({} workspaces)", runtime.topo.len());
    } else if let Some(id) = parse_activated(&value) {
        match mark_active(&mut runtime.topo, id) {
            Some((output, direction)) => {
                log::info!("workspace: activated id={id} on {output} dir={direction}");
                runtime.dirs.insert(output, direction);
            }
            None => log::info!("workspace: activated id={id} (no direction)"),
        }
    } else {
        return;
    }
    if !enabled {
        return;
    }
    let deadline = Instant::now() + Duration::from_millis(debounce);
    if refresh_pending(&mut runtime, &rules, deadline) {
        log::info!("workspace: pending now {:?}", runtime.pending);
        drop(runtime);
        engine.cv.notify_all();
    }
}

fn connect_and_listen(engine: &Engine, state: &WallState) -> std::io::Result<()> {
    let socket = std::env::var("NIRI_SOCKET")
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::NotFound, "NIRI_SOCKET unset"))?;
    let stream = UnixStream::connect(&socket)?;
    let mut writer = stream.try_clone()?;
    writer.write_all(b"\"EventStream\"\n")?;
    writer.flush()?;
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        handle_line(engine, state, &line?);
    }
    Ok(())
}

pub(super) fn reader_loop(engine: &Arc<Engine>, state: &Arc<WallState>) {
    loop {
        match connect_and_listen(engine, state) {
            Ok(()) => log::warn!("workspace: niri event stream ended, reconnecting"),
            Err(error) => log::warn!("workspace: niri event stream error: {error}"),
        }
        std::thread::sleep(RECONNECT_DELAY);
    }
}
