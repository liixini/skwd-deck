use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::Serialize;

use super::{PollAdvance, PollingRoot};

#[derive(Clone, Copy, Default)]
struct ProcessSample {
    user_cpu_us: u64,
    system_cpu_us: u64,
    read_syscalls: u64,
    read_chars: u64,
    storage_read_bytes: u64,
    resident_kib: u64,
    peak_resident_kib: u64,
}

#[derive(Serialize)]
struct SweepReport {
    wall_ms: f64,
    user_cpu_ms: f64,
    system_cpu_ms: f64,
    cpu_percent: f64,
    process_read_syscalls: u64,
    process_rchar_bytes: u64,
    process_storage_read_bytes: u64,
    directory_entries_examined: usize,
    metadata_reads_requested: usize,
    directories_opened: usize,
    advance_calls: usize,
    changed: usize,
    removed: usize,
}

#[derive(Serialize)]
struct ResidentReport {
    unit: &'static str,
    before: u64,
    after_initial: u64,
    after_unchanged: u64,
    growth_after_initial: i64,
    peak: u64,
}

#[derive(Serialize)]
struct BenchmarkReport {
    evidence_kind: String,
    root: String,
    production_entry_budget_per_root: usize,
    measurement_step_budget: usize,
    interval_seconds: u64,
    synthetic_delay_per_advance_us: u64,
    modeled_idle_wakeups_per_hour: f64,
    initial_sweep: SweepReport,
    unchanged_sweep: SweepReport,
    resident_memory: ResidentReport,
}

fn env_number<T>(name: &str, default: T) -> T
where
    T: std::str::FromStr,
{
    std::env::var(name).ok().and_then(|value| value.parse().ok()).unwrap_or(default)
}

fn process_sample() -> ProcessSample {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    let usage_ok = unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } == 0;
    let (user_cpu_us, system_cpu_us) =
        if usage_ok { (timeval_us(usage.ru_utime), timeval_us(usage.ru_stime)) } else { (0, 0) };
    let (read_syscalls, read_chars, storage_read_bytes) = proc_io();
    let (resident_kib, peak_resident_kib) = proc_memory();
    ProcessSample {
        user_cpu_us,
        system_cpu_us,
        read_syscalls,
        read_chars,
        storage_read_bytes,
        resident_kib,
        peak_resident_kib,
    }
}

fn timeval_us(value: libc::timeval) -> u64 {
    u64::try_from(value.tv_sec)
        .unwrap_or(0)
        .saturating_mul(1_000_000)
        .saturating_add(u64::try_from(value.tv_usec).unwrap_or(0))
}

fn proc_io() -> (u64, u64, u64) {
    let Ok(contents) = std::fs::read_to_string("/proc/self/io") else {
        return (0, 0, 0);
    };
    (
        proc_value(&contents, "syscr"),
        proc_value(&contents, "rchar"),
        proc_value(&contents, "read_bytes"),
    )
}

fn proc_memory() -> (u64, u64) {
    let Ok(contents) = std::fs::read_to_string("/proc/self/status") else {
        return (0, 0);
    };
    (proc_value(&contents, "VmRSS"), proc_value(&contents, "VmHWM"))
}

fn proc_value(contents: &str, key: &str) -> u64 {
    contents
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            (name == key).then(|| value.split_whitespace().next()?.parse().ok()).flatten()
        })
        .unwrap_or(0)
}

fn measured_sweep(
    root: &mut PollingRoot,
    step_budget: usize,
    delay: Duration,
    max_calls: usize,
) -> SweepReport {
    let before = process_sample();
    let started = Instant::now();
    let mut calls = 0;
    let delta = loop {
        assert!(calls < max_calls, "polling sweep exceeded {max_calls} advance calls");
        calls += 1;
        let result = root.advance(step_budget);
        if !delay.is_zero() {
            std::thread::sleep(delay);
        }
        match result {
            PollAdvance::Pending => {}
            PollAdvance::Complete(delta) => break delta,
            PollAdvance::Failed(error) => panic!("poll failed: {error}"),
        }
    };
    let elapsed = started.elapsed();
    let after = process_sample();
    let filesystem = root.last_sweep_metrics();
    let user_cpu_us = after.user_cpu_us.saturating_sub(before.user_cpu_us);
    let system_cpu_us = after.system_cpu_us.saturating_sub(before.system_cpu_us);
    let wall_ms = elapsed.as_secs_f64() * 1000.0;
    let cpu_percent = if wall_ms > 0.0 {
        (user_cpu_us.saturating_add(system_cpu_us) as f64 / 1000.0) / wall_ms * 100.0
    } else {
        0.0
    };
    SweepReport {
        wall_ms,
        user_cpu_ms: user_cpu_us as f64 / 1000.0,
        system_cpu_ms: system_cpu_us as f64 / 1000.0,
        cpu_percent,
        process_read_syscalls: after.read_syscalls.saturating_sub(before.read_syscalls),
        process_rchar_bytes: after.read_chars.saturating_sub(before.read_chars),
        process_storage_read_bytes: after
            .storage_read_bytes
            .saturating_sub(before.storage_read_bytes),
        directory_entries_examined: filesystem.entries_examined,
        metadata_reads_requested: filesystem.metadata_reads,
        directories_opened: filesystem.directories_opened,
        advance_calls: calls,
        changed: delta.changed.len(),
        removed: delta.removed.len(),
    }
}

#[test]
#[ignore = "manual filesystem polling benchmark"]
fn polling_fallback_benchmark() {
    let root = PathBuf::from(
        std::env::var("SKWD_POLL_BENCH_ROOT").expect("SKWD_POLL_BENCH_ROOT is required"),
    );
    assert!(root.is_dir(), "benchmark root is not a directory: {}", root.display());
    let production_budget = env_number("SKWD_POLL_BENCH_BUDGET", super::ENTRY_BUDGET_PER_ROOT);
    let step_budget = env_number("SKWD_POLL_BENCH_STEP_BUDGET", production_budget).max(1);
    let interval_seconds = env_number("SKWD_POLL_BENCH_INTERVAL_SECONDS", 60_u64).max(1);
    let delay_us = env_number("SKWD_POLL_BENCH_ENTRY_DELAY_US", 0_u64);
    let max_calls = env_number("SKWD_POLL_BENCH_MAX_CALLS", 20_000_000_usize);
    let evidence_kind =
        std::env::var("SKWD_POLL_BENCH_EVIDENCE").unwrap_or_else(|_| String::from("unclassified"));
    let before = process_sample();
    let mut polling = PollingRoot::new(root.clone(), String::from("benchmark"));
    let delay = Duration::from_micros(delay_us);
    let initial = measured_sweep(&mut polling, step_budget, delay, max_calls);
    let after_initial = process_sample();
    let unchanged = measured_sweep(&mut polling, step_budget, delay, max_calls);
    let after_unchanged = process_sample();
    assert_eq!(unchanged.changed, 0, "unchanged sweep emitted changed paths");
    assert_eq!(unchanged.removed, 0, "unchanged sweep emitted removed paths");
    let report = BenchmarkReport {
        evidence_kind,
        root: root.to_string_lossy().into_owned(),
        production_entry_budget_per_root: production_budget,
        measurement_step_budget: step_budget,
        interval_seconds,
        synthetic_delay_per_advance_us: delay_us,
        modeled_idle_wakeups_per_hour: 3600.0 / interval_seconds as f64,
        initial_sweep: initial,
        unchanged_sweep: unchanged,
        resident_memory: ResidentReport {
            unit: "KiB",
            before: before.resident_kib,
            after_initial: after_initial.resident_kib,
            after_unchanged: after_unchanged.resident_kib,
            growth_after_initial: after_initial.resident_kib as i64 - before.resident_kib as i64,
            peak: after_unchanged.peak_resident_kib,
        },
    };
    println!(
        "SKWD_POLL_BENCH_JSON={}",
        serde_json::to_string(&report).expect("serialize benchmark report")
    );
}
