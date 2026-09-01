use super::*;

#[test]
fn progress_from_duration() {
    assert_eq!(tinier_progress("out_time_us=2500000", Some(10_000)), Some(25));
    assert_eq!(tinier_progress("out_time_us=11000000", Some(10_000)), Some(99));
    assert_eq!(tinier_progress("progress=end", None), Some(99));
    assert_eq!(tinier_progress("out_time_us=10", None), None);
}

#[test]
fn result_fans_out() {
    let work = TinierWork::new("/videos/same.mp4");
    let first = work.subscribe();
    let second = work.subscribe();
    work.finish(Ok(()));
    assert_eq!(first.recv().unwrap(), Ok(()));
    assert_eq!(second.recv().unwrap(), Ok(()));
    assert_eq!(work.subscribe().recv().unwrap(), Ok(()));
}

#[test]
fn task_id_stable_opaque() {
    let first = tinier_task_id("/private/videos/a.mp4");
    assert_eq!(first, tinier_task_id("/private/videos/a.mp4"));
    assert_ne!(first, tinier_task_id("/other/videos/a.mp4"));
    assert!(first.starts_with("tinier:"));
    assert!(!first.contains("private"));
}

#[test]
fn task_status_names_file() {
    let work = TinierWork::new("/private/videos/a.mp4");
    let task = tinier_task_status(&work, 42, "Encoding");
    assert_eq!(task.progress, 42);
    assert_eq!(task.total, 100);
    assert!(task.capabilities.stop);
    assert_eq!(task.detail, "Encoding: a.mp4");
}
