use std::collections::{BTreeMap, VecDeque};
use std::fmt::Write;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

const RECENT_CAP: usize = 14;
const MAX_TRACKED_METHODS: usize = 64;

pub struct Stats {
    started: Instant,
    conns: AtomicU64,
    rpc_total: AtomicU64,
    events: AtomicU64,
    applies: AtomicU64,
    thumbs: AtomicU64,
    errors: AtomicU64,
    by_method: Mutex<BTreeMap<String, u64>>,
    task: Mutex<String>,
    last_applied: Mutex<String>,
    recent: Mutex<VecDeque<String>>,
}

impl Stats {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
            conns: AtomicU64::new(0),
            rpc_total: AtomicU64::new(0),
            events: AtomicU64::new(0),
            applies: AtomicU64::new(0),
            thumbs: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            by_method: Mutex::new(BTreeMap::new()),
            task: Mutex::new("starting".to_string()),
            last_applied: Mutex::new("(none)".to_string()),
            recent: Mutex::new(VecDeque::with_capacity(RECENT_CAP)),
        }
    }

    fn stamp(&self) -> String {
        let secs = self.started.elapsed().as_secs();
        format!("{:02}:{:02}", secs / 60, secs % 60)
    }

    fn push(&self, line: String) {
        if let Ok(mut recent) = self.recent.lock() {
            if recent.len() == RECENT_CAP {
                recent.pop_front();
            }
            recent.push_back(line);
        }
    }

    pub fn conn_open(&self) {
        self.conns.fetch_add(1, Ordering::Relaxed);
    }

    pub fn rpc(&self, method: &str) {
        self.rpc_total.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut map) = self.by_method.lock() {
            if map.len() >= MAX_TRACKED_METHODS && !map.contains_key(method) {
                *map.entry("other".to_string()).or_default() += 1;
            } else {
                *map.entry(method.to_string()).or_default() += 1;
            }
        }
        let stamp = self.stamp();
        self.push(format!("{stamp}  rpc   {method}"));
    }

    pub fn event(&self, name: &str) {
        self.events.fetch_add(1, Ordering::Relaxed);
        let stamp = self.stamp();
        self.push(format!("{stamp}  event {name}"));
    }

    pub fn applied(&self, kind: &str, what: &str) {
        self.applies.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut last) = self.last_applied.lock() {
            *last = format!("{kind}: {what}");
        }
    }

    pub fn thumb(&self) {
        self.thumbs.fetch_add(1, Ordering::Relaxed);
    }

    pub fn error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_task(&self, task: impl Into<String>) {
        if let Ok(mut guard) = self.task.lock() {
            *guard = task.into();
        }
    }

    pub fn counters_json(&self) -> serde_json::Value {
        serde_json::json!({
            "uptime_s": self.started.elapsed().as_secs(),
            "rpc": self.rpc_total.load(Ordering::Relaxed),
            "events": self.events.load(Ordering::Relaxed),
            "applies": self.applies.load(Ordering::Relaxed),
            "thumbs": self.thumbs.load(Ordering::Relaxed),
            "errors": self.errors.load(Ordering::Relaxed),
            "conns": self.conns.load(Ordering::Relaxed),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn banner(
        &self,
        rss: u64,
        wall_rss: u64,
        wall_n: usize,
        fleet_rss: u64,
        fleet: usize,
        scanner_rss: u64,
        vram: u64,
        subs: usize,
    ) -> String {
        let up = self.started.elapsed().as_secs();
        let task = self.task.lock().map(|guard| guard.clone()).unwrap_or_default();
        let last = self.last_applied.lock().map(|guard| guard.clone()).unwrap_or_default();
        let by_method = self
            .by_method
            .lock()
            .map(|map| {
                map.iter().map(|(key, val)| format!("{key} {val}")).collect::<Vec<_>>().join(", ")
            })
            .unwrap_or_default();
        let recent = self
            .recent
            .lock()
            .map(|list| {
                list.iter().rev().map(|line| format!("    {line}")).collect::<Vec<_>>().join("\n")
            })
            .unwrap_or_default();

        let mut out = String::new();
        let _ = writeln!(out, "===== skwd-walld status - up {}m{:02}s =====", up / 60, up % 60);
        let _ = writeln!(out, "  task       : {task}");
        let _ = writeln!(out, "  last apply : {last}");
        let mem = skwd_wall_core::diag::mem_breakdown();
        let _ = writeln!(
            out,
            "  ram        : walld {rss} MB (pss {} / dirty {}) | wallpaper {wall_n} = {wall_rss} MB | transitions {fleet} = {fleet_rss} MB | scanner {scanner_rss} MB | total {} MB",
            mem.pss_kb / 1024,
            mem.private_dirty_kb / 1024,
            rss + wall_rss + fleet_rss + scanner_rss
        );
        #[cfg(feature = "obs-heap")]
        let _ = writeln!(
            out,
            "  heap       : {:.1} MB live (peak {:.1}) | {} allocs",
            skwd_wall_core::countalloc::live_bytes() as f64 / 1e6,
            skwd_wall_core::countalloc::peak_bytes() as f64 / 1e6,
            skwd_wall_core::countalloc::alloc_count(),
        );
        let _ = writeln!(out, "  gpu vram   : renderers {vram} MB");
        let _ = writeln!(
            out,
            "  clients    : subscribers {subs} | conns {}",
            self.conns.load(Ordering::Relaxed)
        );
        let _ = writeln!(
            out,
            "  counters   : rpc {} | events {} | applies {} | thumbs {} | errors {}",
            self.rpc_total.load(Ordering::Relaxed),
            self.events.load(Ordering::Relaxed),
            self.applies.load(Ordering::Relaxed),
            self.thumbs.load(Ordering::Relaxed),
            self.errors.load(Ordering::Relaxed),
        );
        let _ = writeln!(out, "  rpc kinds  : {by_method}");
        let _ = writeln!(out, "  recent (newest first):");
        let _ = write!(out, "{recent}");
        out
    }
}

mod tests;
