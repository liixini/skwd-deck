use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use skwd_wall_core::WallState;
use tokio::io::AsyncWriteExt;
use wall_proto::TaskState;

use crate::infrastructure::stats::Stats;
use crate::infrastructure::tasks::TaskRegistry;

const SCAN_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const PREVIEW_TIMEOUT: Duration = Duration::from_secs(2 * 60);
const REMOTE_TIMEOUT: Duration = Duration::from_secs(10 * 60);
fn scanner_bin() -> std::path::PathBuf {
    skwd_wall_core::paths::sibling_bin("skwd-wall-scan")
}

fn scanner_log() -> Option<std::fs::File> {
    let path = skwd_wall_core::paths::cache_dir().join("skwd-wall-scan.log");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let file = std::fs::OpenOptions::new().create(true).append(true).mode(0o600).open(path).ok()?;
    let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
    Some(file)
}

fn attach_scanner_log(command: &mut std::process::Command) {
    match scanner_log() {
        Some(file) => {
            if let Ok(stdout) = file.try_clone() {
                command.stdout(stdout);
            }
            command.stderr(file);
        }
        None => {
            command.stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        }
    }
}

pub(super) fn scanner_args(debug: bool, extra: &[&str], request_id: Option<&str>) -> Vec<String> {
    debug
        .then_some("--debug")
        .into_iter()
        .chain(request_id.into_iter().flat_map(|id| ["--scan-request-id", id]))
        .chain(extra.iter().copied())
        .map(String::from)
        .collect()
}

pub(super) fn apply_scan_limits(command: &mut std::process::Command, max_jobs: usize) {
    command.env("SKWD_SCAN_THREADS", max_jobs.clamp(1, 32).to_string());
}

pub(crate) fn spawn_scan(
    state: &Arc<WallState>,
    debug: bool,
    extra: &[&str],
    request_id: Option<&str>,
    tracking: Option<(Arc<TaskRegistry>, Arc<Stats>)>,
) {
    let binary = scanner_bin();
    let mut command = crate::infrastructure::proc::tool(&binary);
    command.args(scanner_args(debug, extra, request_id));
    apply_scan_limits(&mut command, state.config().max_thumb_jobs());
    command.stdin(std::process::Stdio::null());
    attach_scanner_log(&mut command);
    supervise_scan(&binary, command, state, scanner_timeout(extra), tracking);
}

pub(super) fn supervise_scan(
    binary: &Path,
    mut command: std::process::Command,
    state: &Arc<WallState>,
    timeout: Duration,
    tracking: Option<(Arc<TaskRegistry>, Arc<Stats>)>,
) {
    match command.spawn() {
        Ok(mut child) => {
            let pid = child.id();
            state.scanner().set_scanner_pid(pid);
            log::info!("spawned scanner {} (pid {pid})", binary.display());
            let state = Arc::clone(state);
            std::thread::spawn(move || {
                let (task_state, detail) = match wait_bounded(&mut child, timeout) {
                    Ok(WaitOutcome::Exited(status)) if status.success() => {
                        (TaskState::Completed, "Scan completed".to_string())
                    }
                    Ok(WaitOutcome::Exited(status)) => {
                        log::warn!("scanner pid {pid} exited with {status}");
                        (TaskState::Failed, format!("Scanner exited with {status}"))
                    }
                    Ok(WaitOutcome::TimedOut) => {
                        log::warn!("scanner pid {pid} exceeded {timeout:?} and was killed");
                        (TaskState::Failed, format!("Scan timed out after {timeout:?}"))
                    }
                    Err(error) => {
                        log::warn!("scanner pid {pid} wait failed: {error}");
                        (TaskState::Failed, format!("Scanner wait failed: {error}"))
                    }
                };
                state.scanner().set_scanner_pid(0);
                if let Some((tasks, stats)) = tracking
                    && tasks.finish_if_active("scan", task_state, detail)
                {
                    stats.set_task("idle");
                }
            });
        }
        Err(error) => {
            log::warn!("failed to spawn scanner {}: {error}", binary.display());
            if let Some((tasks, stats)) = tracking
                && tasks.finish_if_active(
                    "scan",
                    TaskState::Failed,
                    format!("Failed to start scanner: {error}"),
                )
            {
                stats.set_task("idle");
            }
        }
    }
}

pub(super) fn spawn_remote_thumbnails(source: &str, jobs: &[(String, String)]) {
    if jobs.is_empty() {
        return;
    }
    let binary = scanner_bin();
    let mut command = crate::infrastructure::proc::tool_async(&binary);
    command
        .arg("--remote-thumb")
        .arg(source)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let mut payload = String::new();
    for (id, url) in jobs {
        payload.push_str(id);
        payload.push('\t');
        payload.push_str(url);
        payload.push('\n');
    }
    match crate::infrastructure::proc::spawn(&mut command) {
        Ok(mut child) => {
            crate::infrastructure::proc::runtime().spawn(async move {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(payload.as_bytes()).await;
                    drop(stdin);
                }
                match wait_bounded_async(&mut child, REMOTE_TIMEOUT).await {
                    Ok(WaitOutcome::TimedOut) => {
                        log::warn!("remote thumbnail helper exceeded {REMOTE_TIMEOUT:?}");
                    }
                    Ok(WaitOutcome::Exited(_)) => {}
                    Err(error) => log::warn!("remote thumbnail helper wait failed: {error}"),
                }
            });
        }
        Err(error) => log::warn!("spawn remote-thumb failed: {error}"),
    }
}

pub(super) fn scanner_timeout(extra: &[&str]) -> Duration {
    if extra.iter().any(|argument| matches!(*argument, "--preview" | "--theme")) {
        PREVIEW_TIMEOUT
    } else {
        SCAN_TIMEOUT
    }
}

#[derive(Debug)]
pub(super) enum WaitOutcome {
    Exited(std::process::ExitStatus),
    TimedOut,
}

pub(super) fn wait_bounded(
    child: &mut std::process::Child,
    timeout: Duration,
) -> std::io::Result<WaitOutcome> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(WaitOutcome::Exited(status));
        }
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            let _ = child.kill();
            child.wait()?;
            return Ok(WaitOutcome::TimedOut);
        };
        std::thread::sleep(remaining.min(Duration::from_millis(20)));
    }
}

async fn wait_bounded_async(
    child: &mut tokio::process::Child,
    timeout: Duration,
) -> std::io::Result<WaitOutcome> {
    if let Ok(result) = tokio::time::timeout(timeout, child.wait()).await {
        result.map(WaitOutcome::Exited)
    } else {
        let _ = child.start_kill();
        child.wait().await.map(|_| WaitOutcome::TimedOut)
    }
}
