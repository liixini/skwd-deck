use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;

use skwd_wall_core::WallState;

use super::model::WorkspaceInfo;
use super::runtime::{Engine, ingest_snapshot};

const EVENT_PREFIXES: [&str; 8] = [
    "workspace",
    "focusedmon",
    "moveworkspace",
    "createworkspace",
    "destroyworkspace",
    "renameworkspace",
    "monitoradded",
    "monitorremoved",
];

pub(super) fn relevant_event(line: &str) -> bool {
    EVENT_PREFIXES.iter().any(|prefix| line.starts_with(prefix))
}

fn sock_dir() -> std::io::Result<std::path::PathBuf> {
    let not_found = |msg: &str| std::io::Error::new(std::io::ErrorKind::NotFound, msg.to_string());
    let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .map_err(|_| not_found("HYPRLAND_INSTANCE_SIGNATURE unset"))?;
    let runtime =
        std::env::var("XDG_RUNTIME_DIR").map_err(|_| not_found("XDG_RUNTIME_DIR unset"))?;
    Ok(std::path::PathBuf::from(runtime).join("hypr").join(sig))
}

fn query(dir: &std::path::Path, cmd: &str) -> std::io::Result<String> {
    let mut sock = UnixStream::connect(dir.join(".socket.sock"))?;
    sock.write_all(cmd.as_bytes())?;
    let mut out = String::new();
    sock.read_to_string(&mut out)?;
    Ok(out)
}

pub(super) fn build_topology(
    monitors_json: &str,
    workspaces_json: &str,
) -> Option<HashMap<u64, WorkspaceInfo>> {
    let mons: serde_json::Value = serde_json::from_str(monitors_json).ok()?;
    let wss: serde_json::Value = serde_json::from_str(workspaces_json).ok()?;
    let mut active: HashMap<String, i64> = HashMap::new();
    for mon in mons.as_array()? {
        let Some(name) = mon.get("name").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let ws =
            mon.pointer("/activeWorkspace/id").and_then(serde_json::Value::as_i64).unwrap_or(0);
        active.insert(name.to_string(), ws);
    }
    let mut topo = HashMap::new();
    for ws in wss.as_array()? {
        let Some(id) = ws.get("id").and_then(serde_json::Value::as_i64) else {
            continue;
        };
        if id < 1 {
            continue;
        }
        let Some(monitor) = ws.get("monitor").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let name = ws
            .get("name")
            .and_then(serde_json::Value::as_str)
            .filter(|name| !name.is_empty() && *name != id.to_string())
            .map(str::to_string);
        topo.insert(
            id.unsigned_abs(),
            WorkspaceInfo {
                output: monitor.to_string(),
                idx: id.unsigned_abs(),
                name,
                active: active.get(monitor) == Some(&id),
            },
        );
    }
    Some(topo)
}

fn refresh(engine: &Engine, state: &WallState, dir: &std::path::Path) {
    let (mons, wss) = match (query(dir, "j/monitors"), query(dir, "j/workspaces")) {
        (Ok(mons), Ok(wss)) => (mons, wss),
        (Err(err), _) | (_, Err(err)) => {
            log::warn!("workspace: hyprland query failed: {err}");
            return;
        }
    };
    match build_topology(&mons, &wss) {
        Some(topo) => ingest_snapshot(engine, state, topo),
        None => log::warn!("workspace: hyprland topology parse failed"),
    }
}

fn listen(engine: &Engine, state: &WallState) -> std::io::Result<()> {
    let dir = sock_dir()?;
    refresh(engine, state, &dir);
    let stream = UnixStream::connect(dir.join(".socket2.sock"))?;
    for line in BufReader::new(stream).lines() {
        if relevant_event(&line?) {
            refresh(engine, state, &dir);
        }
    }
    Ok(())
}

pub(super) fn reader_loop(engine: &Arc<Engine>, state: &Arc<WallState>) {
    loop {
        match listen(engine, state) {
            Ok(()) => log::warn!("workspace: hyprland event stream ended, reconnecting"),
            Err(err) => log::warn!("workspace: hyprland event stream error: {err}"),
        }
        std::thread::sleep(super::runtime::RECONNECT_DELAY);
    }
}
