use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::sync::Arc;

use skwd_wall_core::WallState;

use super::model::WorkspaceInfo;
use super::runtime::{Engine, ingest_snapshot};

const VDM_DEST: &str = "org.kde.KWin";
const VDM_PATH: &str = "/VirtualDesktopManager";
const VDM_IFACE: &str = "org.kde.KWin.VirtualDesktopManager";

pub(super) fn is_desktop_signal(line: &str) -> bool {
    line.contains("member=currentChanged")
        || line.contains("member=currentDesktopChanged")
        || line.contains("member=desktopCreated")
        || line.contains("member=desktopRemoved")
}

fn dbus_get(prop: &str) -> Option<String> {
    let out = crate::infrastructure::proc::tool("dbus-send")
        .args([
            "--session",
            "--print-reply",
            &format!("--dest={VDM_DEST}"),
            VDM_PATH,
            "org.freedesktop.DBus.Properties.Get",
            &format!("string:{VDM_IFACE}"),
            &format!("string:{prop}"),
        ])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

pub(super) fn parse_desktops_reply(text: &str) -> Vec<(u32, String, String)> {
    let mut out = Vec::new();
    let mut pos: Option<u32> = None;
    let mut id: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(num) = line.strip_prefix("uint32 ") {
            pos = num.trim().parse().ok();
            id = None;
        } else if let Some(text) =
            line.strip_prefix("string \"").and_then(|rest| rest.strip_suffix('"'))
        {
            let Some(idx) = pos else {
                continue;
            };
            match id.take() {
                None => id = Some(text.to_string()),
                Some(prev) => {
                    out.push((idx, prev, text.to_string()));
                    pos = None;
                }
            }
        }
    }
    out
}

pub(super) fn parse_current_reply(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        line.trim()
            .strip_prefix("variant")
            .map(str::trim)
            .and_then(|rest| rest.strip_prefix("string \""))
            .and_then(|rest| rest.strip_suffix('"'))
            .map(str::to_string)
    })
}

pub(super) fn build_topology(
    desktops: &[(u32, String, String)],
    current: &str,
    outputs: &[String],
) -> HashMap<u64, WorkspaceInfo> {
    let mut topo = HashMap::new();
    for (idx, out) in outputs.iter().enumerate() {
        for (pos, id, name) in desktops {
            topo.insert(
                u64::from(*pos) * 1024 + idx as u64,
                WorkspaceInfo {
                    output: out.clone(),
                    idx: u64::from(*pos) + 1,
                    name: Some(name.clone()).filter(|name| !name.is_empty()),
                    active: id == current,
                },
            );
        }
    }
    topo
}

fn refresh(engine: &Engine, state: &WallState) {
    let desktops = dbus_get("desktops").map(|text| parse_desktops_reply(&text)).unwrap_or_default();
    if desktops.is_empty() {
        log::warn!("workspace: kwin virtual desktop list query failed");
        return;
    }
    let Some(current) = dbus_get("current").and_then(|text| parse_current_reply(&text)) else {
        log::warn!("workspace: kwin current desktop query failed");
        return;
    };
    let outputs = skwd_wall_core::outputs::names();
    if outputs.is_empty() {
        return;
    }
    ingest_snapshot(engine, state, build_topology(&desktops, &current, &outputs));
}

fn listen(engine: &Engine, state: &WallState) -> std::io::Result<()> {
    refresh(engine, state);
    let mut child = crate::infrastructure::proc::tool("dbus-monitor")
        .args([
            "--session",
            &format!("type='signal',interface='{VDM_IFACE}'"),
            "type='signal',interface='org.kde.KWin',member='currentDesktopChanged'",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Ok(());
    };
    for line in BufReader::new(stdout).lines() {
        if is_desktop_signal(&line?) {
            refresh(engine, state);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    Ok(())
}

pub(super) fn reader_loop(engine: &Arc<Engine>, state: &Arc<WallState>) {
    loop {
        match listen(engine, state) {
            Ok(()) => log::warn!("workspace: kwin dbus monitor ended, restarting"),
            Err(err) => log::warn!("workspace: kwin dbus monitor error: {err}"),
        }
        std::thread::sleep(super::runtime::RECONNECT_DELAY);
    }
}
