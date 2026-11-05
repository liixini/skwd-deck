pub fn local_time(secs: i64) -> libc::tm {
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        let time = secs as libc::time_t;
        libc::localtime_r(&raw const time, &raw mut tm);
        tm
    }
}

pub fn seconds_until_local_hour(secs: i64, hour: i32) -> u64 {
    let mut target = local_time(secs);
    if target.tm_hour >= hour {
        target.tm_mday += 1;
    }
    target.tm_hour = hour;
    target.tm_min = 0;
    target.tm_sec = 0;
    target.tm_isdst = -1;
    let next = unsafe { libc::mktime(&raw mut target) };
    next.saturating_sub(secs).max(1) as u64
}

#[cfg(test)]
#[path = "clock_tests.rs"]
mod tests;
