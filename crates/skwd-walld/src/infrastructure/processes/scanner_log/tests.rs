use std::io::Read;

#[test]
fn inherited_log_at_file_limit_does_not_kill_helper() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("scanner.log");
    std::fs::File::create(&path).unwrap().set_len(256 * 1024 * 1024).unwrap();
    let (reader, writer) = std::io::pipe().unwrap();
    let mut command = std::process::Command::new("/bin/sh");
    command.args(["-c", "ulimit -f 1; head -c 4096 /dev/zero >&2; printf scan-completed"]);
    command.stdout(writer.try_clone().unwrap()).stderr(writer);
    let mut child = command.spawn().unwrap();
    drop(command);
    super::drain(reader, &path).unwrap();
    assert!(child.wait().unwrap().success());
    assert!(std::fs::read(&path).unwrap().ends_with(b"scan-completed"));
    assert_eq!(
        std::fs::metadata(root.path().join("scanner.log.1")).unwrap().len(),
        256 * 1024 * 1024
    );
}

#[test]
fn verbose_helper_logs_rotate_while_draining() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("scanner.log");
    let noise = std::io::repeat(b'x').take(skwd_log::ROTATE_BYTES * 5);
    super::drain(noise.chain(&b"last diagnostic"[..]), &path).unwrap();
    assert!(std::fs::read(&path).unwrap().ends_with(b"last diagnostic"));
    let files: Vec<_> = std::fs::read_dir(root.path())
        .unwrap()
        .map(Result::unwrap)
        .filter(|entry| entry.path().extension().is_none_or(|ext| ext != "lock"))
        .collect();
    assert_eq!(files.len(), skwd_log::ROTATE_GENERATIONS + 1);
    for file in files {
        assert!(file.metadata().unwrap().len() <= skwd_log::ROTATE_BYTES);
        assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
    }
}
