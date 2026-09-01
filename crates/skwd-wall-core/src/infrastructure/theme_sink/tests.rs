#![cfg(test)]

use super::*;

#[test]
fn sink_registry_resolution() {
    let mut names: Vec<&str> = SINKS.iter().map(|sink| sink.name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), SINKS.len());

    assert_eq!(SINKS.iter().filter(|sink| sink.arm.is_some()).count(), 1);
    assert!(active("bridge").arm.is_some());
    assert!(active("noctalia").arm.is_none());
    assert!(active("dms").arm.is_none());

    assert!(active("noctalia").restore_stale.is_some());
    assert!(active("dms").restore_stale.is_some());
    assert!(active("bridge").restore_stale.is_none());

    assert_eq!(active("noctalia").name, "noctalia");
    assert_eq!(active("dms").name, "dms");
    assert_eq!(active("bridge").name, "bridge");
    assert_eq!(active("matugen").name, "bridge");
    assert_eq!(active("nonsense").name, "bridge");
}
