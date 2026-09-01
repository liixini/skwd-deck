use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use wall_proto::{LibraryWatchRootStatus, LibraryWatchStatus, ev, library_watch_mode};

use crate::backend::events::EventPublisher;

use super::polling;

struct PendingScan {
    roots: Vec<String>,
}

pub(super) struct RuntimeStatus {
    current: LibraryWatchStatus,
    pending: HashMap<String, PendingScan>,
    pending_by_root: HashMap<String, BTreeSet<String>>,
    last_requested: HashMap<String, u64>,
    last_converged: HashMap<String, u64>,
    polling_roots: BTreeSet<String>,
    recovering_roots: BTreeSet<String>,
}

impl Default for RuntimeStatus {
    fn default() -> Self {
        Self {
            current: native("native library watch is active", None),
            pending: HashMap::new(),
            pending_by_root: HashMap::new(),
            last_requested: HashMap::new(),
            last_converged: HashMap::new(),
            polling_roots: BTreeSet::new(),
            recovering_roots: BTreeSet::new(),
        }
    }
}

impl RuntimeStatus {
    pub(super) fn unavailable(&mut self, detail: &str) -> LibraryWatchStatus {
        self.pending.clear();
        self.pending_by_root.clear();
        self.polling_roots.clear();
        self.recovering_roots.clear();
        self.current = LibraryWatchStatus {
            ok: false,
            degraded: true,
            mode: library_watch_mode::UNAVAILABLE.to_string(),
            detail: detail.to_string(),
            ..LibraryWatchStatus::default()
        };
        self.current.clone()
    }

    pub(super) fn native(&mut self, detail: &str) -> LibraryWatchStatus {
        self.pending.clear();
        self.pending_by_root.clear();
        self.polling_roots.clear();
        self.recovering_roots.clear();
        self.current = native(detail, None);
        self.current.clone()
    }

    pub(super) fn polling(
        &mut self,
        roots: &[polling::PollingRoot],
        interval: Duration,
    ) -> LibraryWatchStatus {
        self.polling_roots = roots.iter().map(|root| path_key(&root.path)).collect();
        let mut rows = roots
            .iter()
            .map(|root| LibraryWatchRootStatus {
                path: path_key(&root.path),
                mode: library_watch_mode::POLLING.to_string(),
                native_error: Some(root.reason.clone()),
                last_completed_sweep_unix_ms: root.last_completed_sweep_unix_ms,
                last_scan_requested_unix_ms: None,
                last_successful_convergence_unix_ms: None,
                pending_scans: 0,
                last_poll_error: root.last_error.clone(),
            })
            .collect::<Vec<_>>();
        for path in &self.recovering_roots {
            if !rows.iter().any(|root| root.path == *path) {
                rows.push(LibraryWatchRootStatus {
                    path: path.clone(),
                    mode: library_watch_mode::RECOVERING.to_string(),
                    ..LibraryWatchRootStatus::default()
                });
            }
        }
        let failed = roots.iter().any(|root| root.last_error.is_some());
        self.current = LibraryWatchStatus {
            ok: !failed,
            degraded: true,
            mode: if roots.is_empty() {
                library_watch_mode::RECOVERING
            } else {
                library_watch_mode::POLLING
            }
            .to_string(),
            detail: if failed {
                String::from(
                    "bounded polling fallback is active but at least one root could not be read",
                )
            } else if roots.is_empty() {
                String::from("native library watch recovered; hand-off scan is pending")
            } else {
                String::from("native library watch failed; bounded polling fallback is active")
            },
            interval_seconds: Some(interval.as_secs()),
            entry_budget_per_root: Some(polling::ENTRY_BUDGET_PER_ROOT),
            last_successful_convergence_unix_ms: None,
            roots: rows,
        };
        self.refresh();
        self.current.clone()
    }

    pub(super) fn register_scan(
        &mut self,
        request_id: &str,
        roots: &[PathBuf],
        recovering: &[PathBuf],
    ) -> LibraryWatchStatus {
        let requested = unix_ms();
        let root_keys = roots.iter().map(|path| path_key(path)).collect::<BTreeSet<_>>();
        for path in recovering {
            self.recovering_roots.insert(path_key(path));
        }
        for path in &root_keys {
            self.pending_by_root.entry(path.clone()).or_default().insert(request_id.to_string());
            self.last_requested.insert(path.clone(), requested);
            if !self.current.roots.iter().any(|root| root.path == *path) {
                self.current.roots.push(LibraryWatchRootStatus {
                    path: path.clone(),
                    mode: if self.recovering_roots.contains(path) {
                        library_watch_mode::RECOVERING
                    } else {
                        library_watch_mode::POLLING
                    }
                    .to_string(),
                    ..LibraryWatchRootStatus::default()
                });
            }
        }
        self.pending
            .insert(request_id.to_string(), PendingScan { roots: root_keys.into_iter().collect() });
        self.refresh();
        self.current.clone()
    }

    pub(super) fn complete_scan(&mut self, request_id: &str) -> Option<LibraryWatchStatus> {
        let scan = self.pending.remove(request_id)?;
        let completed = unix_ms();
        for path in scan.roots {
            let no_pending = self.pending_by_root.get_mut(&path).is_some_and(|pending| {
                pending.remove(request_id);
                pending.is_empty()
            });
            if no_pending {
                self.pending_by_root.remove(&path);
                self.last_converged.insert(path.clone(), completed);
                self.recovering_roots.remove(&path);
            }
        }
        self.refresh();
        if self.current.mode == library_watch_mode::NATIVE {
            self.current.last_successful_convergence_unix_ms = Some(completed);
        }
        Some(self.current.clone())
    }

    pub(super) fn snapshot(&self) -> LibraryWatchStatus {
        self.current.clone()
    }

    fn refresh(&mut self) {
        self.current.roots.retain(|root| {
            self.polling_roots.contains(&root.path) || self.recovering_roots.contains(&root.path)
        });
        for root in &mut self.current.roots {
            root.mode = if self.recovering_roots.contains(&root.path) {
                library_watch_mode::RECOVERING
            } else {
                library_watch_mode::POLLING
            }
            .to_string();
            root.pending_scans = self.pending_by_root.get(&root.path).map_or(0, BTreeSet::len);
            root.last_scan_requested_unix_ms = self.last_requested.get(&root.path).copied();
            root.last_successful_convergence_unix_ms = self.last_converged.get(&root.path).copied();
        }
        let polling = !self.polling_roots.is_empty();
        let recovering = !self.recovering_roots.is_empty();
        if polling {
            self.current.mode = library_watch_mode::POLLING.to_string();
            self.current.degraded = true;
        } else if recovering {
            self.current.mode = library_watch_mode::RECOVERING.to_string();
            self.current.ok = true;
            self.current.degraded = true;
            self.current.detail =
                String::from("native library watch recovered; hand-off scan is pending");
        } else if !self.current.roots.is_empty() || !self.pending.is_empty() {
            self.current.mode = library_watch_mode::RECOVERING.to_string();
            self.current.ok = true;
            self.current.degraded = true;
        } else if self.current.mode != library_watch_mode::UNAVAILABLE {
            let previous = self.current.last_successful_convergence_unix_ms;
            self.current =
                native("native library watch recovered and hand-off scan converged", previous);
        }
        self.current.last_successful_convergence_unix_ms = if self.current.roots.is_empty() {
            self.current.last_successful_convergence_unix_ms
        } else {
            self.current
                .roots
                .iter()
                .map(|root| root.last_successful_convergence_unix_ms)
                .collect::<Option<Vec<_>>>()
                .and_then(|values| values.into_iter().min())
        };
    }
}

static RUNTIME: LazyLock<Mutex<RuntimeStatus>> =
    LazyLock::new(|| Mutex::new(RuntimeStatus::default()));
static PERSIST_REVISION: AtomicU64 = AtomicU64::new(0);
static PERSIST_LOCK: Mutex<()> = Mutex::new(());

pub(super) fn snapshot() -> LibraryWatchStatus {
    runtime().snapshot()
}

pub(super) fn record_native(publisher: &dyn EventPublisher, detail: &str, retain_file: bool) {
    let current = runtime().native(detail);
    if retain_file {
        persist(&current);
    } else {
        schedule_persist(Persist::Remove);
    }
    publish(publisher, &current);
}

pub(super) fn record_unavailable(
    publisher: &dyn EventPublisher,
    detail: &str,
) -> LibraryWatchStatus {
    let current = runtime().unavailable(detail);
    persist(&current);
    publish(publisher, &current);
    current
}

pub(super) fn record_polling(
    publisher: &dyn EventPublisher,
    roots: &[polling::PollingRoot],
    interval: Duration,
) -> LibraryWatchStatus {
    let current = runtime().polling(roots, interval);
    persist(&current);
    publish(publisher, &current);
    current
}

pub(super) fn register_scan(
    publisher: &dyn EventPublisher,
    roots: &[polling::PollingRoot],
    interval: Duration,
    request_id: &str,
    correlated_roots: &[PathBuf],
    recovering_roots: &[PathBuf],
) {
    let current = {
        let mut runtime = runtime();
        runtime.polling(roots, interval);
        runtime.register_scan(request_id, correlated_roots, recovering_roots)
    };
    persist(&current);
    publish(publisher, &current);
}

pub(super) fn complete_scan(
    publisher: &dyn EventPublisher,
    request_id: &str,
) -> Option<LibraryWatchStatus> {
    let current = runtime().complete_scan(request_id)?;
    persist(&current);
    publish(publisher, &current);
    Some(current)
}

fn runtime() -> std::sync::MutexGuard<'static, RuntimeStatus> {
    RUNTIME.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn native(detail: &str, convergence: Option<u64>) -> LibraryWatchStatus {
    LibraryWatchStatus {
        ok: true,
        degraded: false,
        mode: library_watch_mode::NATIVE.to_string(),
        detail: detail.to_string(),
        last_successful_convergence_unix_ms: convergence,
        ..LibraryWatchStatus::default()
    }
}

fn path_key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |value| u64::try_from(value.as_millis()).unwrap_or(u64::MAX))
}

fn persist(current: &LibraryWatchStatus) {
    let Ok(value) = serde_json::to_value(current) else {
        return;
    };
    schedule_persist(Persist::Write(value));
}

enum Persist {
    Write(serde_json::Value),
    Remove,
}

fn schedule_persist(operation: Persist) {
    let revision = PERSIST_REVISION.fetch_add(1, Ordering::AcqRel).saturating_add(1);
    let task = move || persist_if_current(revision, operation);
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            handle.spawn_blocking(task);
        }
        Err(_) => task(),
    }
}

fn persist_if_current(revision: u64, operation: Persist) {
    let _guard = PERSIST_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if revision != PERSIST_REVISION.load(Ordering::Acquire) {
        return;
    }
    let path = super::watch_status_path();
    match operation {
        Persist::Write(value) => super::write_status(&path, &value),
        Persist::Remove => {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn publish(publisher: &dyn EventPublisher, current: &LibraryWatchStatus) {
    if let Ok(value) = serde_json::to_value(current) {
        publisher.publish(ev::WATCH_STATUS, value);
    }
}
