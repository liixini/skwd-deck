#![cfg(test)]

use super::*;

#[cfg(target_os = "linux")]
#[test]
fn socket_name_identity() {
    let name = socket_name();
    let base = format!("skwd-wall-v2.{}", unsafe { libc::getuid() });
    assert!(name == base || name.starts_with(&format!("{base}.")));
}
