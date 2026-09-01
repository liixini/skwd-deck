#![cfg(test)]
use super::*;

#[test]
fn stat_parses_plain_comm() {
    let text = "1234 (skwd-walld) S 1 1234 1234 0 -1 4194304 100 0 0 0 42 8 0 0 20 0 5 0 100 0 0";
    let stat = parse_stat(text).unwrap();
    assert_eq!(stat.name, "skwd-walld");
    assert_eq!(stat.ppid, 1);
    assert_eq!(stat.cpu_ticks, 50);
    assert_eq!(stat.threads, 5);
}

#[test]
fn stat_comm_with_parens() {
    let text = "99 (a (weird) name) R 42 99 99 0 -1 0 0 0 0 0 7 3 0 0 20 0 2 0 1 0 0";
    let stat = parse_stat(text).unwrap();
    assert_eq!(stat.name, "a (weird) name");
    assert_eq!(stat.ppid, 42);
    assert_eq!(stat.cpu_ticks, 10);
    assert_eq!(stat.threads, 2);
}

#[test]
fn stat_rejects_garbage() {
    assert!(parse_stat("").is_none());
    assert!(parse_stat("1234 (x) S").is_none());
}

#[test]
fn smi_parses_pid_mem_pairs() {
    let text = "1234, 163\n5678, 20\nnot a line\n";
    assert_eq!(parse_smi(text), vec![(1234, 163), (5678, 20)]);
}

#[test]
fn tracked_selection() {
    let mk = |name: &str, ppid: u32| StatLine { name: name.into(), ppid, cpu_ticks: 0, threads: 1 };
    assert!(tracked(10, 10, &mk("anything", 1)));
    assert!(tracked(20, 10, &mk("ffmpeg", 10)));
    assert!(tracked(30, 10, &mk("skwd-wall-vk", 1)));
    assert!(tracked(40, 10, &mk("awww", 1)));
    assert!(!tracked(50, 10, &mk("firefox", 1)));
}

#[tokio::test]
async fn wake_preempts_idle_recheck() {
    wake();
    let woken =
        crate::infrastructure::wake::wake_or_timeout(&WAKE, Duration::from_secs(3600)).await;
    assert!(!woken);
}

#[test]
fn own_stat_parses_live() {
    let text = std::fs::read_to_string("/proc/self/stat").unwrap();
    let stat = parse_stat(&text).unwrap();
    assert!(!stat.name.is_empty());
    assert!(stat.threads >= 1);
}
