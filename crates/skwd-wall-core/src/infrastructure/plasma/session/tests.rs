use super::*;
use std::os::unix::net::UnixStream;

#[test]
fn unresponsive_compositor_does_not_hang_or_claim_plasma() {
    let (client, _server) = UnixStream::pair().unwrap();
    let connection = Connection::from_socket(client).unwrap();
    let start = Instant::now();
    assert_eq!(probe(&connection), None);
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn disconnected_compositor_does_not_claim_plasma() {
    let (client, server) = UnixStream::pair().unwrap();
    let connection = Connection::from_socket(client).unwrap();
    drop(server);
    assert_eq!(probe(&connection), None);
}
