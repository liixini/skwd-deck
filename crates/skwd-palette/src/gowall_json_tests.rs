use super::*;

#[test]
fn options_cover_names() {
    let options = options();
    assert_eq!(options.len(), crate::gowall::names().len());
    assert!(options.iter().all(|option| option["mode"] == option["label"]));
}
