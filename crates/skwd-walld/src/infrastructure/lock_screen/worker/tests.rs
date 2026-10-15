use super::*;

#[tokio::test]
async fn worker_reads_reply() {
    let mut command = crate::infrastructure::proc::tool_async("printf");
    command.arg("wallpaper-0000000000000001.png\n");
    assert_eq!(execute(command).await.unwrap(), b"wallpaper-0000000000000001.png\n");
}

#[tokio::test]
async fn worker_rejects_output() {
    for bytes in [String::new(), "x".repeat(REPLY_BYTES + 1)] {
        let mut command = crate::infrastructure::proc::tool_async("sh");
        command.arg("-c").arg("printf '%s' \"$1\"; exit 1").arg("worker").arg(bytes);
        assert!(execute(command).await.is_err());
    }
}

#[tokio::test(start_paused = true)]
async fn worker_timeout_reaps() {
    let mut command = crate::infrastructure::proc::tool_async("sleep");
    command.arg("60");
    let error = execute(command).await.unwrap_err();
    assert!(error.to_string().contains("timed out"));
}

#[tokio::test]
async fn worker_reaps_bad_reply() {
    let directory = tempfile::tempdir().unwrap();
    let pid_file = directory.path().join("pid");
    let mut command = crate::infrastructure::proc::tool_async("sh");
    command
        .arg("-c")
        .arg("printf '%s' \"$$\" > \"$1\"; printf '%s' \"$2\"; exec sleep 60")
        .arg("worker")
        .arg(&pid_file)
        .arg("x".repeat(REPLY_BYTES + 1));
    assert!(execute(command).await.is_err());
    let pid = std::fs::read_to_string(pid_file).unwrap();
    assert!(!PathBuf::from(format!("/proc/{pid}")).exists());
}
