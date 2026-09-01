use super::*;

#[test]
fn row_round_trip_defaults() {
    let wire = serde_json::json!({"output": "DP-1", "idx": 3, "name": "web", "active": true});
    let row: WorkspaceRow = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(row.name.as_deref(), Some("web"));
    assert_eq!(serde_json::to_value(&row).unwrap(), wire);

    let unnamed: WorkspaceRow =
        serde_json::from_value(serde_json::json!({"output": "DP-1", "idx": 1, "name": null}))
            .unwrap();
    assert_eq!(unnamed.name, None);
    assert!(!unnamed.active);
}
