use std::collections::BTreeMap;
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

        if wayland_socket(&environment).is_some() {
            apply_environment(&environment);
            log_ready(&environment, started.elapsed());
            return;
        }

        match discovered {
            Some(display) => {
                let seen_at = socket_seen_at.get_or_insert_with(Instant::now);
                if seen_at.elapsed() >= SOCKET_FALLBACK_GRACE {
                    environment.insert("WAYLAND_DISPLAY".into(), display);
                    apply_environment(&environment);
                    log_ready(&environment, started.elapsed());
                    return;
                }
            }
            None => socket_seen_at = None,
        }

        thread::sleep(POLL_INTERVAL);
    }
}

fn log_ready(environment: &Environment, elapsed: Duration) {
    let desktop = environment.get("XDG_CURRENT_DESKTOP").map_or("?", String::as_str);
    let display = environment.get("WAYLAND_DISPLAY").map_or("?", String::as_str);
    log::info!(
        "Wayland session ready after {} ms: desktop={desktop} wayland={display}",
        elapsed.as_millis()
    );
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
