#![cfg(test)]

use super::*;

#[test]
fn debug_flag_wins() {
    assert_eq!(level_from(true, Some("error")), LogLevel::Debug);
    assert_eq!(level_from(true, None), LogLevel::Debug);
}

#[test]
fn env_level_filter() {
    assert_eq!(level_from(false, Some("trace")), LogLevel::Trace);
    assert_eq!(level_from(false, Some("debug")), LogLevel::Debug);
    assert_eq!(level_from(false, Some("warn")), LogLevel::Warn);
    assert_eq!(level_from(false, Some("error")), LogLevel::Error);
    assert_eq!(level_from(false, Some("verbose")), LogLevel::Info);
    assert_eq!(level_from(false, None), LogLevel::Info);
}
