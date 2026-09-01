use super::*;

#[test]
fn socket_suffix() {
    assert!(socket_path().ends_with("skwd-wall-v2/wall.sock"));
}
