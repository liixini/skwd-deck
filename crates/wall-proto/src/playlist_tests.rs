use super::*;

#[test]
fn rows_round_trip_defaults() {
    let wire = serde_json::json!({
        "id": 3, "name": "Chill", "kind": "smart", "source": "tag:calm",
        "order": "sequential", "dwell": 120, "position": 1, "count": 7
    });
    let row: PlaylistRow = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(row.source.as_deref(), Some("tag:calm"));
    assert_eq!(serde_json::to_value(&row).unwrap(), wire);

    let assignment: PlaylistAssign =
        serde_json::from_value(serde_json::json!({"output": "DP-1", "id": 3})).unwrap();
    assert_eq!((assignment.output.as_str(), assignment.id), ("DP-1", 3));

    let null_source: PlaylistRow = serde_json::from_value(serde_json::json!({
        "id": 1, "name": "x", "kind": "curated", "source": null,
        "order": "shuffle", "dwell": 600, "position": 0, "count": 0
    }))
    .unwrap();
    assert_eq!(null_source.source, None);
}
