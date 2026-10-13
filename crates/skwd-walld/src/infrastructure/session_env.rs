use std::collections::BTreeMap;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

const IMPORT_KEYS: &[&str] = &[
    "DBUS_SESSION_BUS_ADDRESS",
    "DESKTOP_SESSION",
    "DISPLAY",
    "HYPRLAND_INSTANCE_SIGNATURE",
    "KDE_FULL_SESSION",
    "NIRI_SOCKET",
    "SWAYSOCK",
    "WAYLAND_DISPLAY",
    "XAUTHORITY",
    "XDG_CURRENT_DESKTOP",
    "XDG_RUNTIME_DIR",
    "XDG_SESSION_DESKTOP",
    "XDG_SESSION_TYPE",
];
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const SOCKET_FALLBACK_GRACE: Duration = Duration::from_secs(2);
const NIRI_DESKTOP: &str = "niri";
const DESKTOP_KEY: &str = "XDG_CURRENT_DESKTOP";
const MISSING_IPC: &str = "-";
const NIRI_SOCKET_PREFIX: &str = "niri.";
const NIRI_SOCKET_KEY: &str = "NIRI_SOCKET";
const HYPRLAND_SIGNATURE_KEY: &str = "HYPRLAND_INSTANCE_SIGNATURE";
const NIRI_SOCKET_SUFFIX: &str = ".sock";
const HYPRLAND_DIR: &str = "hypr";
const HYPRLAND_LOCK: &str = "hyprland.lock";
const PROC_DIR: &str = "/proc";

type Environment = BTreeMap<String, String>;

pub(crate) fn wait_for_wayland() {
    let started = Instant::now();
    let mut socket_seen_at = None;
    log::info!("waiting for the Wayland session before restoring wallpapers");

    loop {
        let mut environment = current_environment();
        let runtime = runtime_dir(&environment);
        let discovered = runtime.as_deref().and_then(discover_wayland_socket);

        if environment.contains_key("WAYLAND_DISPLAY") || discovered.is_some() {
            overlay(&mut environment, manager_environment());
        }
        if let Some(runtime) = runtime {
            environment
                .entry("XDG_RUNTIME_DIR".into())
                .or_insert_with(|| runtime.to_string_lossy().into_owned());
        }

        if let Some(ready) = ready_environment(environment.clone()) {
            finish(&ready, started.elapsed());
            return;
        }

        match discovered {
            Some(display) => {
                let seen_at = socket_seen_at.get_or_insert_with(Instant::now);
                if seen_at.elapsed() >= SOCKET_FALLBACK_GRACE {
                    environment.insert("WAYLAND_DISPLAY".into(), display);
                    if let Some(ready) = ready_environment(environment) {
                        finish(&ready, started.elapsed());
                        return;
                    }
                }
            }
            None => socket_seen_at = None,
        }

        thread::sleep(POLL_INTERVAL);
    }
}

fn finish(environment: &Environment, elapsed: Duration) {
    apply_environment(environment);
    let value = |key: &str, missing| environment.get(key).map_or(missing, String::as_str);
    log::info!(
        "Wayland session ready after {} ms: desktop={} wayland={} niri={} hyprland={}",
        elapsed.as_millis(),
        value(DESKTOP_KEY, "?"),
        value("WAYLAND_DISPLAY", "?"),
        value(NIRI_SOCKET_KEY, MISSING_IPC),
        value(HYPRLAND_SIGNATURE_KEY, MISSING_IPC),
    );
}

fn ready_environment(mut environment: Environment) -> Option<Environment> {
    wayland_socket(&environment)?;
    adopt_compositor_ipc(&mut environment);
    let niri = environment.get(DESKTOP_KEY).is_some_and(|desktop| {
        desktop.split(':').any(|name| name.eq_ignore_ascii_case(NIRI_DESKTOP))
    });
    (!niri || environment.get(NIRI_SOCKET_KEY).is_some_and(|socket| is_socket(Path::new(socket))))
        .then_some(environment)
}

fn adopt_compositor_ipc(environment: &mut Environment) {
    let Some(runtime) = runtime_dir(environment) else { return };
    let Some(display) = wayland_display_path(environment)
        .and_then(|path| path.file_name()?.to_str().map(str::to_owned))
    else {
        return;
    };
    let niri_prefix = format!("{NIRI_SOCKET_PREFIX}{display}.");
    let current = environment.get(NIRI_SOCKET_KEY).is_some_and(|socket| {
        let path = Path::new(socket);
        path.parent() == Some(runtime.as_path()) && live_niri_socket(path, &niri_prefix)
    });
    if !current && let Some(socket) = discover_niri_socket(&runtime, &display) {
        environment.insert(NIRI_SOCKET_KEY.into(), socket);
    } else if !current {
        environment.remove(NIRI_SOCKET_KEY);
    }
    if !environment.contains_key(HYPRLAND_SIGNATURE_KEY)
        && let Some(signature) = discover_hyprland_signature(&runtime, &display)
    {
        environment.insert(HYPRLAND_SIGNATURE_KEY.into(), signature);
    }
}

fn discover_niri_socket(runtime: &Path, display: &str) -> Option<String> {
    let prefix = format!("{NIRI_SOCKET_PREFIX}{display}.");
    std::fs::read_dir(runtime).ok()?.filter_map(Result::ok).find_map(|entry| {
        let path = entry.path();
        live_niri_socket(&path, &prefix).then(|| path.to_string_lossy().into_owned())
    })
}

fn live_niri_socket(path: &Path, prefix: &str) -> bool {
    let pid = path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix(prefix))
        .and_then(|name| name.strip_suffix(NIRI_SOCKET_SUFFIX))
        .and_then(|pid| pid.parse::<u32>().ok());
    pid.is_some_and(alive) && is_socket(path) && niri_socket_ready(path)
}

fn niri_socket_ready(path: &Path) -> bool {
    let name = path.as_os_str().as_bytes();
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    if name.len() >= address.sun_path.len() {
        return false;
    }
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (slot, byte) in address.sun_path.iter_mut().zip(name) {
        *slot = *byte as libc::c_char;
    }

    let fd = unsafe {
        libc::socket(libc::AF_UNIX, libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC, 0)
    };
    if fd < 0 {
        return false;
    }
    let socket = unsafe { OwnedFd::from_raw_fd(fd) };
    unsafe {
        libc::connect(
            socket.as_raw_fd(),
            std::ptr::from_ref(&address).cast(),
            std::mem::size_of_val(&address) as libc::socklen_t,
        ) == 0
    }
}

fn discover_hyprland_signature(runtime: &Path, display: &str) -> Option<String> {
    std::fs::read_dir(runtime.join(HYPRLAND_DIR)).ok()?.filter_map(Result::ok).find_map(|entry| {
        let lock = std::fs::read_to_string(entry.path().join(HYPRLAND_LOCK)).ok()?;
        let mut lines = lock.lines();
        let pid = lines.next()?.trim().parse::<u32>().ok()?;
        let owns_display = lines.next()?.trim() == display;
        (owns_display && alive(pid)).then(|| entry.file_name().into_string().ok())?
    })
}

fn alive(pid: u32) -> bool {
    Path::new(PROC_DIR).join(pid.to_string()).exists()
}

fn current_environment() -> Environment {
    IMPORT_KEYS
        .iter()
        .filter_map(|key| std::env::var(key).ok().map(|value| ((*key).into(), value)))
        .collect()
}

fn manager_environment() -> Environment {
    crate::infrastructure::proc::tool("systemctl")
        .args(["--user", "show-environment"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map_or_else(Environment::new, |output| {
            parse_manager_environment(&String::from_utf8_lossy(&output.stdout))
        })
}

fn parse_manager_environment(output: &str) -> Environment {
    output
        .lines()
        .filter_map(|line| line.split_once('='))
        .filter(|(key, _)| IMPORT_KEYS.contains(key))
        .map(|(key, value)| (key.into(), value.into()))
        .collect()
}

fn overlay(environment: &mut Environment, newer: Environment) {
    environment.extend(newer);
}

fn runtime_dir(environment: &Environment) -> Option<PathBuf> {
    environment.get("XDG_RUNTIME_DIR").map(PathBuf::from).or_else(|| {
        let path = PathBuf::from(format!("/run/user/{}", unsafe { libc::geteuid() }));
        path.is_dir().then_some(path)
    })
}

fn wayland_socket(environment: &Environment) -> Option<PathBuf> {
    let path = wayland_display_path(environment)?;
    is_socket(&path).then_some(path)
}

fn wayland_display_path(environment: &Environment) -> Option<PathBuf> {
    let display = environment.get("WAYLAND_DISPLAY")?;
    Some(if Path::new(display).is_absolute() {
        PathBuf::from(display)
    } else {
        runtime_dir(environment)?.join(display)
    })
}

fn discover_wayland_socket(runtime: &Path) -> Option<String> {
    discover_wayland_socket_with(runtime, is_socket)
}

fn discover_wayland_socket_with(runtime: &Path, socket: impl Fn(&Path) -> bool) -> Option<String> {
    let mut displays = std::fs::read_dir(runtime)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let suffix = name.strip_prefix("wayland-")?;
            let index = suffix.parse::<u32>().ok()?;
            socket(&entry.path()).then_some((index, name))
        })
        .collect::<Vec<_>>();
    displays.sort_by_key(|(index, _)| *index);
    displays.pop().map(|(_, name)| name)
}

fn is_socket(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|metadata| metadata.file_type().is_socket())
}

fn apply_environment(environment: &Environment) {
    for (key, value) in environment {
        unsafe {
            std::env::set_var(key, value);
        }
    }
}

mod tests;
