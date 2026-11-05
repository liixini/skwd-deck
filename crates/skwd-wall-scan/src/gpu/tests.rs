use std::fs::File;
use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use super::watch_peer;

#[test]
fn disconnect_stops_worker() {
    if std::env::var("SKWD_TEST_GPU_PEER").as_deref() == Ok("1") {
        let mut peer = File::from(std::io::stdin().as_fd().try_clone_to_owned().unwrap());
        watch_peer(peer.try_clone().unwrap()).unwrap();
        peer.write_all(b"ready").unwrap();
        loop {
            std::thread::park();
        }
    }
    let (mut parent, child_socket) = UnixStream::pair().unwrap();
    parent.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "gpu::tests::disconnect_stops_worker", "--nocapture"])
        .env("SKWD_TEST_GPU_PEER", "1")
        .stdin(Stdio::from(OwnedFd::from(child_socket)))
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let pid = child.id();
    let mut ready = [0; 5];
    parent.read_exact(&mut ready).unwrap();
    assert_eq!(&ready, b"ready");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || sender.send(child.wait().unwrap()).unwrap());
    parent.shutdown(Shutdown::Both).unwrap();
    let status = receiver.recv_timeout(Duration::from_secs(3)).unwrap_or_else(|error| {
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
        panic!("worker shutdown: {error}");
    });
    assert!(status.success());
}
