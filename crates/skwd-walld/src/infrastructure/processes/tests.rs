use super::scanner::WaitOutcome;
use super::scanner_args;
use std::sync::Arc;

fn wait_for_task(
    tasks: &crate::infrastructure::tasks::TaskRegistry,
    expected: wall_proto::TaskState,
) -> wall_proto::TaskStatus {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        if let Some(task) = tasks.list().into_iter().find(|task| task.id == "scan")
            && task.state == expected
        {
            return task;
        }
        assert!(std::time::Instant::now() < deadline, "scan task did not reach {expected:?}");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn scanner_args_debug_first() {
    assert_eq!(
        scanner_args(true, &["--paths", "/wall/a.png"], Some("watch-9")),
        ["--debug", "--scan-request-id", "watch-9", "--paths", "/wall/a.png"]
    );
    assert_eq!(scanner_args(false, &["--recolor"], None), ["--recolor"]);
}

#[test]
fn scanner_job_limit_env() {
    let mut command = std::process::Command::new("unused");
    super::scanner::apply_scan_limits(&mut command, 5);
    let value = command
        .get_envs()
        .find(|(name, _)| *name == std::ffi::OsStr::new("SKWD_SCAN_THREADS"))
        .and_then(|(_, value)| value)
        .map(|value| value.to_string_lossy().into_owned());
    assert_eq!(value.as_deref(), Some("5"));
}

#[test]
fn scanner_deadlines_match_job_kind() {
    assert_eq!(
        super::scanner::scanner_timeout(&["--preview", "video:key", "/video"]),
        std::time::Duration::from_secs(120)
    );
    assert_eq!(
        super::scanner::scanner_timeout(&["--paths", "/wall/a.png"]),
        std::time::Duration::from_secs(30 * 60)
    );
}

#[test]
fn bounded_wait_kills_stuck_helper() {
    let mut command = std::process::Command::new("/bin/sh");
    command.arg("-c").arg("sleep 30");
    let mut child = command.spawn().unwrap();
    assert!(matches!(
        super::scanner::wait_bounded(&mut child, std::time::Duration::from_millis(20)).unwrap(),
        WaitOutcome::TimedOut
    ));
    assert!(child.try_wait().unwrap().is_some());
}

#[test]
fn successful_exit_finishes_unreported_scan() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({}));
    let (state, events, stats) = crate::testenv::harness();
    let tasks = Arc::new(crate::infrastructure::tasks::TaskRegistry::new(events));
    tasks.update(wall_proto::TaskStatus::running("scan", "scan", "Scanning wallpapers"));
    super::scanner::supervise_scan(
        std::path::Path::new("/bin/true"),
        std::process::Command::new("/bin/true"),
        &state,
        std::time::Duration::from_secs(1),
        Some((Arc::clone(&tasks), Arc::clone(&stats))),
    );
    let task = wait_for_task(&tasks, wall_proto::TaskState::Completed);
    assert_eq!(task.detail, "Scan completed");
    assert!(stats.banner(0, 0, 0, 0, 0, 0, 0, 0).contains("task       : idle"));
}

#[test]
fn exit_fallback_preserves_reported_completion() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({}));
    let (state, events, stats) = crate::testenv::harness();
    let tasks = Arc::new(crate::infrastructure::tasks::TaskRegistry::new(events));
    tasks.update(wall_proto::TaskStatus::running("scan", "scan", "Scanning wallpapers"));
    let mut command = std::process::Command::new("/bin/sh");
    command.arg("-c").arg("sleep 0.05");
    super::scanner::supervise_scan(
        std::path::Path::new("/bin/sh"),
        command,
        &state,
        std::time::Duration::from_secs(1),
        Some((Arc::clone(&tasks), Arc::clone(&stats))),
    );
    tasks.finish("scan", wall_proto::TaskState::Completed, "3 wallpapers found");
    std::thread::sleep(std::time::Duration::from_millis(100));
    let task = tasks.list().into_iter().find(|task| task.id == "scan").unwrap();
    assert_eq!(task.state, wall_proto::TaskState::Completed);
    assert_eq!(task.detail, "3 wallpapers found");
}
