use std::os::unix::fs::PermissionsExt;

use super::{remove_stale_socket, secure_socket_dir};

#[test]
fn ipc_connection_limit() {
    use std::io::{BufRead, BufReader, Write};
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    let (_g, root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({}));
    let (state, subs, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &subs, &stats);
    let path = root.join("run/ipc-limit.sock");
    let _ = std::fs::remove_file(&path);
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    let runtime = crate::testenv::runtime();
    let serving = runtime.spawn(super::serve(listener, ctx));

    let status_roundtrip = |stream: &mut std::os::unix::net::UnixStream| {
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        stream.write_all(b"{\"method\":\"status\",\"id\":1}\n").ok();
        let mut line = String::new();
        let bytes = BufReader::new(stream.try_clone().unwrap()).read_line(&mut line).unwrap_or(0);
        (bytes, line)
    };

    let mut held = Vec::new();
    for connection in 0..super::MAX_IPC_CONNECTIONS {
        let mut stream = std::os::unix::net::UnixStream::connect(&path).unwrap();
        let (bytes, line) = status_roundtrip(&mut stream);
        assert!(bytes > 0 && line.contains("\"result\""), "connection {connection} unserved");
        held.push(stream);
    }

    let rejected_before = super::REJECTED_CONNECTIONS.load(Ordering::Acquire);
    let mut extra = std::os::unix::net::UnixStream::connect(&path).unwrap();
    extra.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut line = String::new();
    let bytes = BufReader::new(&mut extra).read_line(&mut line).unwrap_or(0);
    assert_eq!(bytes, 0, "{line}");
    assert_eq!(super::REJECTED_CONNECTIONS.load(Ordering::Acquire), rejected_before + 1);

    drop(held.pop().unwrap());
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let mut retry = std::os::unix::net::UnixStream::connect(&path).unwrap();
        let (bytes, line) = status_roundtrip(&mut retry);
        if bytes > 0 {
            assert!(line.contains("\"result\""));
            break;
        }
        assert!(std::time::Instant::now() < deadline, "permit not reusable");
        std::thread::sleep(Duration::from_millis(10));
    }

    serving.abort();
    drop(runtime);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn socket_dir_0700() {
    let base = crate::testenv::tmp("sock");
    let dir = base.join("skwd");
    secure_socket_dir(&dir).expect("secure fresh dir");
    let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700);
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn socket_dir_mkdir_fail() {
    let base = crate::testenv::tmp("sockfail");
    let file = base.join("iamafile");
    std::fs::write(&file, b"x").unwrap();
    let err = secure_socket_dir(&file.join("under-a-file")).expect_err("mkdir under file");
    assert!(err.to_string().contains("create socket dir"));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn socket_dir_mode_migrated() {
    let base = crate::testenv::tmp("sockmode");
    let dir = base.join("shared");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    secure_socket_dir(&dir).expect("secure legacy dir");
    assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn socket_dir_symlink_rejected() {
    use std::os::unix::fs::symlink;

    let base = crate::testenv::tmp("socklink");
    let actual = base.join("actual");
    let link = base.join("skwd");
    std::fs::create_dir_all(&actual).unwrap();
    symlink(&actual, &link).unwrap();
    let error = secure_socket_dir(&link).expect_err("symlink rejected");
    assert!(error.to_string().contains("not a real directory"));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn stale_socket_keeps_file() {
    let base = crate::testenv::tmp("sock-stale");
    let path = base.join("wall.sock");
    std::fs::write(&path, b"keep me").unwrap();
    let error = remove_stale_socket(&path).expect_err("regular file kept");
    assert!(error.to_string().contains("non-socket"));
    assert_eq!(std::fs::read(&path).unwrap(), b"keep me");
    let _ = std::fs::remove_dir_all(&base);
}
