#![cfg(test)]

use skwd_log::proc::proc_kb;

use super::parse_vram;

const SAMPLE: &str = "\
| Processes:                                                                              |
|  GPU   GI   CI              PID   Type   Process name                        GPU Memory |
|        ID   ID                                                               Usage      |
|=========================================================================================|
|    0   N/A  N/A             996      G   /usr/lib/Xorg                            75MiB |
|    0   N/A  N/A            1206      G   /usr/local/bin/niri-blur                448MiB |
|    0   N/A  N/A            3044    C+G   ...rack-uuid=319                          87MiB |
|   1   30%   45C    P8    25W / 350W |    2048MiB / 12288MiB |   skip-summary-row        |";

#[test]
fn vram_sums_pids() {
    assert_eq!(parse_vram(SAMPLE, &[1206]), 448);
    assert_eq!(parse_vram(SAMPLE, &[1206, 3044]), 535);
    assert_eq!(parse_vram(SAMPLE, &[999999]), 0);
    assert_eq!(parse_vram(SAMPLE, &[]), 0);
}

#[test]
fn vram_skips_summary() {
    assert_eq!(parse_vram(SAMPLE, &[2048]), 0);
}

#[test]
fn proc_kb_units() {
    let status = "Name:\tskwd-wall\nVmRSS:\t1536 kB\n";
    assert_eq!(proc_kb(status, "VmRSS:"), 1536);
    assert_eq!(proc_kb(status, "VmSize:"), 0);
}
