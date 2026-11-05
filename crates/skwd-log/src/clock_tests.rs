use super::{local_time, seconds_until_local_hour};

#[test]
fn epoch_has_local_fields() {
    let tm = local_time(0);
    assert!((0..24).contains(&tm.tm_hour));
    assert!((0..60).contains(&tm.tm_min));
}

#[test]
fn next_local_boundary() {
    let now = 1_780_000_000;
    let current = local_time(now);
    let target = if current.tm_hour < 19 { 19 } else { 7 };
    let later = now + seconds_until_local_hour(now, target) as i64;
    let boundary = local_time(later);
    assert_eq!(boundary.tm_hour, target);
    assert_eq!(boundary.tm_min, 0);
    assert_eq!(boundary.tm_sec, 0);
}

#[test]
fn dst_crossing() {
    if std::env::var_os("SKWD_LOG_DST_CHILD").is_none() {
        let binary = std::env::current_exe().unwrap();
        let result = std::process::Command::new(binary)
            .arg("--exact")
            .arg("clock::tests::dst_crossing")
            .env("TZ", "Europe/Stockholm")
            .env("SKWD_LOG_DST_CHILD", "1")
            .output()
            .unwrap();
        let output = String::from_utf8_lossy(&result.stdout);
        assert!(result.status.success(), "{output}");
        assert!(output.contains("1 passed"), "{output}");
        return;
    }
    let hour = 7;
    assert_eq!(seconds_until_local_hour(1_774_742_400, hour), 5 * 60 * 60);
    assert_eq!(seconds_until_local_hour(1_792_886_400, hour), 6 * 60 * 60);
}
