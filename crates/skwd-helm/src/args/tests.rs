#![cfg(test)]

use super::*;

#[test]
fn take_option_flag_positional() {
    let mut args = vec!["--output".into(), "DP-1".into(), "--json".into(), "x".into()];
    assert_eq!(take_option(&mut args, &["--output", "-o"]), Some("DP-1".into()));
    assert!(take_flag(&mut args, &["--json"]));
    assert_eq!(first_positional(&args), Some("x".into()));
    assert!(!take_flag(&mut args, &["--missing"]));
}
