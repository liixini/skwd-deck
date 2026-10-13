use super::*;

#[test]
fn activity_wire_shape() {
    let value = serde_json::json!({"supported":true,"current":"a","activities":[{"id":"a","name":"Work","current":true},{"id":"b","name":"Home","current":false}]});
    let result: ActivitiesResult = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(result).unwrap(), value);
    assert_eq!(ACTIVITY_PARAM, "activity");
}
