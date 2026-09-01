#![cfg(test)]

use super::*;

#[test]
fn deck_status_golden() {
    let actual = deck_status("0.1.0");
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("../tests/golden/status-v1.json")).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn legacy_status_fields() {
    let status = deck_status("9.8.7");
    assert_eq!(status["ok"], true);
    assert_eq!(status["version"], "9.8.7");
}
