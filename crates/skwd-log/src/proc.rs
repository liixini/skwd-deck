#[derive(Debug, Clone, Copy, Default)]
pub struct MemBreakdown {
    pub rss_kb: u64,
    pub vss_kb: u64,
    pub pss_kb: u64,
    pub private_dirty_kb: u64,
}

#[must_use]
pub fn proc_kb(text: &str, key: &str) -> u64 {
    text.lines()
        .find_map(|line| line.strip_prefix(key))
        .and_then(|value| value.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
        .unwrap_or(0)
}

#[must_use]
pub fn sum_proc_kb(text: &str, key: &str) -> u64 {
    text.lines()
        .filter_map(|line| line.strip_prefix(key))
        .filter_map(|value| value.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
        .sum()
}

#[must_use]
pub fn pid_rss_kb(pid: u32) -> u64 {
    std::fs::read_to_string(format!("/proc/{pid}/status"))
        .map_or(0, |text| proc_kb(&text, "VmRSS:"))
}

#[must_use]
pub fn mem_breakdown() -> MemBreakdown {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let rollup = std::fs::read_to_string("/proc/self/smaps_rollup").unwrap_or_default();
    MemBreakdown {
        rss_kb: proc_kb(&status, "VmRSS:"),
        vss_kb: proc_kb(&status, "VmSize:"),
        pss_kb: proc_kb(&rollup, "Pss:"),
        private_dirty_kb: sum_proc_kb(&rollup, "Private_Dirty:"),
    }
}

mod tests;
