use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::lock;

const PROBE_TTL: Duration = Duration::from_secs(60);

pub(super) fn probe_cached(
    cache: &Mutex<HashMap<String, (bool, Instant)>>,
    binary: &str,
    probe: impl FnOnce() -> bool,
) -> bool {
    let entries = lock(cache);
    if let Some((available, checked_at)) = entries.get(binary)
        && checked_at.elapsed() < PROBE_TTL
    {
        return *available;
    }
    drop(entries);
    let available = probe();
    cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(binary.to_string(), (available, Instant::now()));
    available
}

pub(crate) fn cli_available(binary: &str) -> bool {
    cli_probe(binary, "--version")
}

fn cli_probe(binary: &str, flag: &str) -> bool {
    static CACHE: OnceLock<Mutex<HashMap<String, (bool, Instant)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    probe_cached(cache, binary, || {
        Command::new(binary)
            .arg(flag)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    })
}

pub fn backend_available(config: &Config, backend: &str) -> bool {
    match backend {
        "off" | "native" | "static" | "skwd-iris" | "skwd-pywal" | "skwd-wallust" => true,
        "matugen" | "dms" => cli_available("matugen"),
        "wallust" => cli_available("wallust"),
        "iris" => cli_available("iris"),
        "pywal" => cli_probe("wal", "-v"),
        "caelestia" | "end4" => crate::theme_provider::authority_available(config, backend),
        "noctalia" => cli_available(&crate::noctalia::bin(config)),
        _ => false,
    }
}

pub const ALL_BACKENDS: [&str; 14] = wall_proto::THEME_BACKENDS;

pub fn available_backends(config: &Config) -> Vec<&'static str> {
    std::thread::scope(|scope| {
        let handles: Vec<_> = ALL_BACKENDS
            .iter()
            .map(|&backend| (backend, scope.spawn(move || backend_available(config, backend))))
            .collect();
        handles
            .into_iter()
            .filter_map(|(backend, handle)| handle.join().unwrap_or(false).then_some(backend))
            .collect()
    })
}

pub(super) fn resolve_backend(config: &Config) -> String {
    let requested = config.theme().backend();
    if requested == "off" || backend_available(config, &requested) {
        return requested;
    }
    log::warn!("theme backend '{requested}' unavailable, falling back to native");
    "native".to_string()
}

pub fn effective_backend(config: &Config) -> String {
    resolve_backend(config)
}
