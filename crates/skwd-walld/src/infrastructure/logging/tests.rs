#![cfg(test)]

#[test]
fn walld_log_location() {
    let path = super::walld_log_path();
    assert!(path.ends_with("skwd-wall-v2/skwd-walld.log"), "{path:?}");
}
