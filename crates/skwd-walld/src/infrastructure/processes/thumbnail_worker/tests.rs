use super::*;
use crate::infrastructure::proc;

#[test]
fn closing_worker_allows_pending_reports_to_flush() {
    let root = tempfile::tempdir().unwrap();
    let finished = root.path().join("finished");
    let mut command = proc::tool("/bin/sh");
    command
        .arg("-c")
        .arg("read -r request; printf '{}\\n'; read -r end; printf finished > \"$1\"")
        .arg("thumbnail-test")
        .arg(&finished);
    let children = Children::default();
    let mut worker = Worker::spawn(command, &children).unwrap();
    let _: serde_json::Value = worker.exchange(&serde_json::json!({})).unwrap();
    let child = lock(&children)[0].upgrade().unwrap();
    drop(worker);
    assert_eq!(std::fs::read_to_string(finished).unwrap(), "finished");
    assert!(lock(&child).try_wait().unwrap().unwrap().success());
}
