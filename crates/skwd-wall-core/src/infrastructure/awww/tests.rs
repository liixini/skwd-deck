#![cfg(test)]

use super::*;

#[test]
fn resize_modes() {
    assert_eq!(resize_for("fill"), Some("crop"));
    assert_eq!(resize_for("fit"), Some("fit"));
    assert_eq!(resize_for("stretch"), Some("stretch"));
    assert_eq!(resize_for("center"), Some("no"));
    assert_eq!(resize_for("tile"), None);
    assert!(!supports("tile"));
    assert!(supports("center"));
}
