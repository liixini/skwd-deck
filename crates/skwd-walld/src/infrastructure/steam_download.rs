use anyhow::Context;
use skwd_wall_core::lock;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::sync::Mutex;

use serde_json::json;
use wall_proto::ev;

use crate::backend::events::EventPublisher;
use crate::infrastructure::steam;

const MISSING_STEAM_HELPER: &str = "Steam Client support needs the optional skwd-deck-steamworks package (skwd-steam). Install it, or select SteamCMD in Settings > Steam.";

fn steam_bin() -> std::path::PathBuf {
    resolve_steam_bin(std::env::current_exe().ok().as_deref(), std::env::var_os("PATH").as_deref())
        .unwrap_or_else(|| "skwd-steam".into())
}

fn resolve_steam_bin(
    exe: Option<&std::path::Path>,
    search: Option<&std::ffi::OsStr>,
) -> Option<std::path::PathBuf> {
    let sibling = exe.and_then(std::path::Path::parent).map(|dir| dir.join("skwd-steam"));
    sibling
        .into_iter()
        .chain(search.into_iter().flat_map(std::env::split_paths).map(|dir| dir.join("skwd-steam")))
        .find(|path| {
            path.metadata()
                .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        })
}

pub(crate) fn require_steam_helper() -> anyhow::Result<()> {
    anyhow::ensure!(
        resolve_steam_bin(
            std::env::current_exe().ok().as_deref(),
            std::env::var_os("PATH").as_deref()
        )
        .is_some(),
        MISSING_STEAM_HELPER
    );
    Ok(())
}

fn helper_spawn_error(error: &std::io::Error) -> String {
    if error.kind() == std::io::ErrorKind::NotFound {
        MISSING_STEAM_HELPER.to_string()
    } else {
        format!("Cannot start the Steam Client helper (skwd-steam): {error}")
    }
}

pub(crate) fn run_unsubscribe(
    id: &str,
    subscriptions_expected: bool,
    publisher: &dyn EventPublisher,
) {
    let out = crate::infrastructure::proc::tool(steam_bin())
        .arg("unsub")
        .arg(id)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    let ok = out.as_ref().is_ok_and(std::process::ExitStatus::success);
    match &out {
        Ok(_) if ok => log::info!("steam unsub {id}: ok"),
        Ok(status) => {
            log::warn!("steam unsub {id}: helper exited {status} (Steam down or not subscribed)");
        }
        Err(err) => log::warn!("steam unsub {id}: spawn failed: {err}"),
    }
    publisher.publish(
        ev::UNSUBSCRIBED,
        json!({ "id": id, "ok": ok, "warn": !ok && subscriptions_expected }),
    );
}

static STEAM_INFLIGHT: std::sync::LazyLock<Mutex<std::collections::HashSet<String>>> =
    std::sync::LazyLock::new(|| Mutex::new(std::collections::HashSet::new()));

static STEAMCMD_GATE: Mutex<()> = Mutex::new(());

fn steamcmd_serialize(
    publisher: &dyn EventPublisher,
    ids: &[String],
) -> std::sync::MutexGuard<'static, ()> {
    match STEAMCMD_GATE.try_lock() {
        Ok(guard) => guard,
        Err(std::sync::TryLockError::Poisoned(poison)) => poison.into_inner(),
        Err(std::sync::TryLockError::WouldBlock) => {
            for id in ids {
                steam_dl_msg(
                    publisher,
                    id,
                    wall_proto::dl_status::QUEUED,
                    0.0,
                    "queued behind another Steam batch",
                );
            }
            lock(&STEAMCMD_GATE)
        }
    }
}

pub(crate) fn steam_inflight_begin(id: &str) -> bool {
    STEAM_INFLIGHT.lock().is_ok_and(|mut set| set.insert(id.to_string()))
}

pub(crate) fn steam_inflight_end(id: &str) {
    if let Ok(mut set) = STEAM_INFLIGHT.lock() {
        set.remove(id);
    }
}

pub(crate) fn steam_dl_event(
    publisher: &dyn EventPublisher,
    id: &str,
    status: &str,
    progress: f64,
) {
    steam_dl_msg(publisher, id, status, progress, "");
}

fn steam_dl_msg(
    publisher: &dyn EventPublisher,
    id: &str,
    status: &str,
    progress: f64,
    message: &str,
) {
    let payload = wall_proto::DownloadEvent {
        progress: Some(progress),
        message: (!message.is_empty()).then(|| message.to_string()),
        ..wall_proto::DownloadEvent::new(id, status)
    };
    publisher.publish(ev::DOWNLOAD, payload.to_value());
}

fn reconcile_we_item(we_dir: &std::path::Path, id: &str, actual: &std::path::Path) {
    let target = we_dir.join(id);
    if target.exists() || actual.as_os_str().is_empty() || !actual.exists() || actual == target {
        return;
    }
    let _ = std::fs::create_dir_all(we_dir);
    let _ = std::os::unix::fs::symlink(actual, &target);
}

fn finalize_steam_batch(
    publisher: &dyn EventPublisher,
    we_dir: &std::path::Path,
    ids: &[String],
    fail_status: &str,
    fail_message: &str,
    folders: &std::collections::HashMap<String, String>,
) -> bool {
    let mut ok = false;
    for id in ids {
        if let Some(folder) = folders.get(id).filter(|folder| !folder.is_empty()) {
            reconcile_we_item(we_dir, id, std::path::Path::new(folder));
        }
        if we_dir.join(id).is_dir() {
            steam_dl_event(publisher, id, wall_proto::dl_status::DONE, 1.0);
            ok = true;
        } else {
            steam_dl_msg(publisher, id, fail_status, 0.0, fail_message);
        }
    }
    ok
}

pub(crate) fn steam_helper_search(
    params: &steam::SearchParams,
) -> anyhow::Result<steam::SearchPage> {
    require_steam_helper()?;
    let req = json!({
        "query": params.query,
        "query_type": params.query_type,
        "days": params.days,
        "tags": params.tags,
        "excluded_tags": params.excluded_tags,
        "page": params.page,
    })
    .to_string();
    let out = crate::infrastructure::proc::tool(steam_bin())
        .arg("search")
        .arg(&req)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .map_err(|error| anyhow::anyhow!(helper_spawn_error(&error)))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout.lines().rev().find(|line| line.trim_start().starts_with('{')).unwrap_or("");
    if !out.status.success() && line.is_empty() {
        anyhow::bail!(
            "Steam Client helper exited with {}. Check that native Steam is running and signed in.",
            out.status
        );
    }
    steam::parse_helper_search(line, params.page).context("Steam Client search failed")
}

pub(crate) fn run_steamcmd_download(
    publisher: &dyn EventPublisher,
    username: &str,
    install_root: &str,
    we_dir: &std::path::Path,
    ids: &[String],
) -> bool {
    let _serial = steamcmd_serialize(publisher, ids);
    for id in ids {
        steam_dl_event(publisher, id, wall_proto::dl_status::DOWNLOADING, 0.0);
    }
    let args = steam::steamcmd_args(username, install_root, ids);
    let mut cmd = crate::infrastructure::proc::tool("steamcmd");
    cmd.args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) => {
            log::warn!("steam: failed to spawn steamcmd: {err}");
            for id in ids {
                steam_dl_msg(
                    publisher,
                    id,
                    wall_proto::dl_status::ERROR,
                    0.0,
                    "steamcmd not found - install it, or switch to the Steam Client backend in Settings > Steam",
                );
            }
            return false;
        }
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.wait();
        return false;
    };
    let auth_error = pump_steamcmd_progress(publisher, ids, stdout);
    let _ = child.wait();
    let content = std::path::Path::new(install_root).join("steamapps/workshop/content/431960");
    let folders: std::collections::HashMap<String, String> = ids
        .iter()
        .map(|id| (id.clone(), content.join(id).to_string_lossy().into_owned()))
        .collect();
    let (fail_status, fail_message) = steamcmd_fail_message(auth_error, username);
    finalize_steam_batch(publisher, we_dir, ids, fail_status, &fail_message, &folders)
}

fn pump_steamcmd_progress(
    publisher: &dyn EventPublisher,
    ids: &[String],
    stdout: std::process::ChildStdout,
) -> bool {
    let empty = std::collections::HashSet::new();
    let mut current = ids.first().cloned().unwrap_or_default();
    let mut auth_error = false;
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if line.contains("Downloading item") {
            if let Some(id) = steam::extract_workshop_id(&line, ids, &empty) {
                current.clone_from(&id);
                steam_dl_event(publisher, &id, wall_proto::dl_status::DOWNLOADING, 0.0);
            }
        } else if let Some(pct) = steam::extract_percent(&line) {
            steam_dl_event(publisher, &current, wall_proto::dl_status::DOWNLOADING, pct);
        }
        if steam::is_auth_error(&line) {
            auth_error = true;
        }
    }
    auth_error
}

fn steamcmd_fail_message(auth_error: bool, username: &str) -> (&'static str, String) {
    if !auth_error {
        return (
            wall_proto::dl_status::ERROR,
            "Download failed - is steamcmd installed and logged in to a Steam account that owns Wallpaper Engine? Or switch to the Steam Client backend.".to_string(),
        );
    }
    let user = if username.is_empty() { "<your-steam-username>" } else { username };
    (
        wall_proto::dl_status::AUTH_ERROR,
        format!(
            "Steam login required - run once in a terminal: steamcmd +login {user} +quit  (then retry), or switch to the Steam Client backend in Settings > Steam"
        ),
    )
}

pub(crate) fn run_steamworks_download(
    publisher: &dyn EventPublisher,
    we_dir: &std::path::Path,
    ids: &[String],
) -> bool {
    let mut cmd = crate::infrastructure::proc::tool(steam_bin());
    cmd.args(ids)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) => {
            log::warn!("steam: failed to spawn skwd-steam helper: {err}");
            for id in ids {
                steam_dl_msg(
                    publisher,
                    id,
                    wall_proto::dl_status::ERROR,
                    0.0,
                    &helper_spawn_error(&err),
                );
            }
            return false;
        }
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.wait();
        return false;
    };
    let mut folders: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut last_error: Option<String> = None;
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let id = val.get("id").and_then(serde_json::Value::as_str).unwrap_or("");
        let status = val.get("status").and_then(serde_json::Value::as_str).unwrap_or("");
        let progress = val.get("progress").and_then(serde_json::Value::as_f64).unwrap_or(0.0);
        let message = val.get("message").and_then(serde_json::Value::as_str).unwrap_or("");
        if id.is_empty() {
            continue;
        }
        match status {
            "done" => {
                let folder = val.get("folder").and_then(serde_json::Value::as_str).unwrap_or("");
                folders.insert(id.to_string(), folder.to_string());
                steam_dl_event(publisher, id, wall_proto::dl_status::DOWNLOADING, 1.0);
            }
            "error" => {
                if !message.is_empty() {
                    last_error = Some(message.to_string());
                }
            }
            _ => steam_dl_event(publisher, id, status, progress),
        }
    }
    let _ = child.wait();
    let fail_message = last_error.unwrap_or_else(|| {
        "Steam couldn't install the item - make sure Steam is running and logged in to the account that owns Wallpaper Engine (or switch to the steamcmd backend)".to_string()
    });
    finalize_steam_batch(
        publisher,
        we_dir,
        ids,
        wall_proto::dl_status::ERROR,
        &fail_message,
        &folders,
    )
}

mod tests;
