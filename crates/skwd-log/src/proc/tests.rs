#![cfg(test)]

use super::{proc_kb, sum_proc_kb};

#[test]
fn parses_status_lines() {
    let status = "Name:\tx\nVmRSS:\t    1536 kB\nThreads:\t7\n";
    assert_eq!(proc_kb(status, "VmRSS:"), 1536);
    assert_eq!(proc_kb(status, "VmSize:"), 0);
}

#[test]
fn sums_rollup_lines() {
    let rollup = "Private_Dirty:       4 kB\nPrivate_Dirty:       6 kB\nPss:  10 kB\n";
    assert_eq!(sum_proc_kb(rollup, "Private_Dirty:"), 10);
    assert_eq!(proc_kb(rollup, "Pss:"), 10);
}
