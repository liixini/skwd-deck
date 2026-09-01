use std::io::Write;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use tokio::sync::Notify;

use crate::infrastructure::wake::wake_or_timeout;

static WAKE: Notify = Notify::const_new();

pub(crate) fn wake() {
    WAKE.notify_one();
}

#[derive(Debug, PartialEq)]
pub(crate) struct StatLine {
    pub name: String,
    pub ppid: u32,
    pub cpu_ticks: u64,
    pub threads: u64,
}

pub(crate) fn parse_stat(text: &str) -> Option<StatLine> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    let name = text.get(open + 1..close)?.to_string();
    let rest: Vec<&str> = text.get(close + 1..)?.split_whitespace().collect();
    let ppid = rest.get(1)?.parse().ok()?;
    let utime: u64 = rest.get(11)?.parse().ok()?;
    let stime: u64 = rest.get(12)?.parse().ok()?;
    let threads = rest.get(17)?.parse().ok()?;
    Some(StatLine { name, ppid, cpu_ticks: utime + stime, threads })
}

pub(crate) fn parse_smi(text: &str) -> Vec<(u32, u64)> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split(',');
            let pid = parts.next()?.trim().parse().ok()?;
            let mem = parts.next()?.trim().parse().ok()?;
            Some((pid, mem))
        })
        .collect()
}

pub(crate) fn tracked(pid: u32, self_pid: u32, stat: &StatLine) -> bool {
    pid == self_pid || stat.ppid == self_pid || stat.name.starts_with("skwd") || stat.name == "awww"
}

fn page_kb() -> u64 {
    static PAGE_KB: OnceLock<u64> = OnceLock::new();
    *PAGE_KB.get_or_init(|| {
        let size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        if size > 0 { size as u64 / 1024 } else { 4 }
    })
}

fn rss_kb(pid: u32) -> u64 {
    std::fs::read_to_string(format!("/proc/{pid}/statm"))
        .ok()
        .and_then(|text| text.split_whitespace().nth(1)?.parse::<u64>().ok())
        .map_or(0, |pages| pages * page_kb())
}

fn fd_count(pid: u32) -> u64 {
    std::fs::read_dir(format!("/proc/{pid}/fd")).map_or(0, |dir| dir.count() as u64)
}

fn vram_by_pid() -> Vec<(u32, u64)> {
    crate::infrastructure::proc::tool("nvidia-smi")
        .args(["--query-compute-apps=pid,used_memory", "--format=csv,noheader,nounits"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| parse_smi(&String::from_utf8_lossy(&out.stdout)))
        .unwrap_or_default()
}

fn sample(include_vram: bool) -> serde_json::Value {
    let self_pid = std::process::id();
    let vram = if include_vram { vram_by_pid() } else { Vec::new() };
    let mut procs = Vec::new();
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return serde_json::Value::Null;
    };
    for entry in dir.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|name| name.parse::<u32>().ok()) else {
            continue;
        };
        let Some(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .and_then(|text| parse_stat(&text))
        else {
            continue;
        };
        if !tracked(pid, self_pid, &stat) {
            continue;
        }
        let vram_mb = vram.iter().find(|(vp, _)| *vp == pid).map(|(_, mb)| *mb);
        procs.push(serde_json::json!({
            "pid": pid,
            "name": stat.name,
            "rss_kb": rss_kb(pid),
            "fds": fd_count(pid),
            "threads": stat.threads,
            "cpu_ticks": stat.cpu_ticks,
            "vram_mb": vram_mb,
        }));
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |dur| dur.as_secs());
    serde_json::json!({ "ts": ts, "procs": procs })
}

pub(crate) fn vitals_path() -> std::path::PathBuf {
    skwd_wall_core::paths::cache_dir().join("vitals.jsonl")
}

fn append_sample(line: &serde_json::Value) {
    let path = vitals_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    skwd_log::rotate_if_large(&path, skwd_log::ROTATE_BYTES);
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "{line}");
    }
}

pub(crate) fn start(ctx: crate::composition::context::Ctx) {
    tokio::spawn(vitals_loop(ctx.state));
}

async fn vitals_loop(state: Arc<skwd_wall_core::WallState>) {
    let mut last: Option<std::time::Instant> = None;
    loop {
        let poll_state = Arc::clone(&state);
        let Ok((enabled, mins)) = tokio::task::spawn_blocking(move || {
            poll_state.reload_config();
            let cfg = poll_state.config();
            (cfg.vitals_enabled(), cfg.vitals_interval_mins())
        })
        .await
        else {
            break;
        };
        if !enabled {
            last = None;
            wake_or_timeout(&WAKE, crate::composition::bootstrap::IDLE_RECHECK).await;
            continue;
        }
        let interval = Duration::from_secs(mins * 60);
        let due = last.map_or(Duration::ZERO, |at| interval.saturating_sub(at.elapsed()));
        if !due.is_zero() {
            wake_or_timeout(&WAKE, due).await;
            continue;
        }
        let _ = tokio::task::spawn_blocking(|| {
            let line = sample(!skwd_config::on_battery_power());
            if !line.is_null() {
                append_sample(&line);
            }
        })
        .await;
        last = Some(std::time::Instant::now());
    }
}

mod tests;
