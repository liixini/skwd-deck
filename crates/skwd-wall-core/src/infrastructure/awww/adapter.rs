use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Context;

use crate::state::WallState;

const READY_POLL_TRIES: usize = 60;
const READY_POLL_INTERVAL: Duration = Duration::from_millis(50);

static AWWW_ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn resize_for(fill_mode: &str) -> Option<&'static str> {
    match fill_mode {
        "fill" => Some("crop"),
        "fit" => Some("fit"),
        "stretch" => Some("stretch"),
        "center" => Some("no"),
        _ => None,
    }
}

pub fn supports(fill_mode: &str) -> bool {
    resize_for(fill_mode).is_some()
}

fn daemon_running() -> bool {
    Command::new("awww")
        .arg("query")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|st| st.success())
}

pub fn ensure_daemon() -> anyhow::Result<()> {
    if daemon_running() {
        return Ok(());
    }
    let mut daemon = Command::new("awww-daemon");
    daemon.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    if !crate::proc::spawn_reaped(&mut daemon, "awww-daemon") {
        anyhow::bail!("spawn awww-daemon");
    }
    for _ in 0..READY_POLL_TRIES {
        if daemon_running() {
            return Ok(());
        }
        std::thread::sleep(READY_POLL_INTERVAL);
    }
    Ok(())
}

pub fn stop() {
    if !AWWW_ACTIVE.swap(false, Ordering::SeqCst) {
        return;
    }
    let mut cmd = Command::new("awww");
    cmd.arg("kill").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    crate::proc::spawn_reaped(&mut cmd, "awww kill");
}

fn transition_args(state: &WallState) -> Vec<String> {
    let cfg = state.config();
    let mut args = Vec::new();
    let mut push = |flag: &str, key: &str| {
        if let Some(val) = cfg.renderer().awww_arg(key) {
            args.push(flag.to_string());
            args.push(val);
        }
    };
    push("--transition-type", "transitionType");
    push("--transition-step", "transitionStep");
    push("--transition-fps", "transitionFps");
    push("--transition-angle", "transitionAngle");
    push("--transition-pos", "transitionPos");
    push("--transition-bezier", "transitionBezier");
    let dur_ms = cfg
        .renderer()
        .awww_arg("transitionDurationMs")
        .and_then(|val| val.parse::<f64>().ok())
        .filter(|ms| *ms > 0.0)
        .unwrap_or(1000.0);
    args.push("--transition-duration".to_string());
    args.push(format!("{:.3}", dur_ms / 1000.0));
    if let (Some(w), Some(h)) = (
        cfg.renderer().awww_arg("transitionWaveWidth"),
        cfg.renderer().awww_arg("transitionWaveHeight"),
    ) {
        args.push("--transition-wave".to_string());
        args.push(format!("{w},{h}"));
    }
    if cfg.renderer().awww_arg("invertY").as_deref() == Some("true") {
        args.push("--invert-y".to_string());
    }
    args
}

pub fn img_args(state: &WallState, output: &str, path: &str, fill_mode: &str) -> Vec<String> {
    let cfg = state.config();
    let resize = resize_for(fill_mode).unwrap_or("crop");
    let mut args = vec![
        "img".to_string(),
        path.to_string(),
        "--resize".to_string(),
        resize.to_string(),
        "--fill-color".to_string(),
        cfg.display().fill_color(),
        "--filter".to_string(),
        cfg.renderer().awww_filter(),
    ];
    if output != "*" {
        args.push("--outputs".to_string());
        args.push(output.to_string());
    }
    drop(cfg);
    args.extend(transition_args(state));
    args
}

pub fn apply(state: &WallState, output: &str, path: &str, fill_mode: &str) -> anyhow::Result<()> {
    state.renderers().kill_all();
    ensure_daemon()?;
    AWWW_ACTIVE.store(true, Ordering::SeqCst);
    let args = img_args(state, output, path, fill_mode);
    log::debug!("awww apply: awww {}", args.join(" "));
    let status = Command::new("awww")
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("run awww img")?;
    if !status.success() {
        anyhow::bail!("awww img failed ({status}) for {path}; args: {}", args.join(" "));
    }
    state.renderers().set_assignment_for(output, path, &crate::outputs::names());
    Ok(())
}

#[path = "tests.rs"]
mod tests;
