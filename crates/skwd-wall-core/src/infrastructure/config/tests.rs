use super::ConfigStore;

#[test]
fn store_snapshot() {
    let store = ConfigStore::from_root(serde_json::json!({
        "display": { "fillMode": "fit" }
    }));

    assert_eq!(store.read().display().fill_mode(), "fit");
}
