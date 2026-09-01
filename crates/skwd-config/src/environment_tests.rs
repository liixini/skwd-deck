use super::*;

#[test]
fn resolve_tilde() {
    assert!(resolve("~/x").ends_with("/x"));
    assert!(!resolve("~/x").starts_with('~'));
    assert_eq!(resolve("/abs/p"), "/abs/p");
    assert_eq!(resolve("~user/p"), "~user/p");
}
