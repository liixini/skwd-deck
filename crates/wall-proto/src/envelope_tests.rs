use super::*;
use serde_json::json;

fn request(params: serde_json::Value) -> Request {
    Request { method: "x".into(), params, id: 1 }
}

#[test]
fn request_param_defaults() {
    let request = request(json!({
        "name": "hello", "outputs": ["DP-1", 3, "HDMI-A-1"],
        "flag": "yes", "n": "7", "s": 12
    }));
    assert_eq!(request.str_param("name", "fallback"), "hello");
    assert_eq!(request.str_param("missing", "fallback"), "fallback");
    assert_eq!(request.str_array("outputs"), vec!["DP-1", "HDMI-A-1"]);
    assert!(!request.bool_param("flag", false));
    assert_eq!(request.opt_i64("n"), None);
    assert_eq!(request.opt_str("s"), None);
}

#[test]
fn sparse_request_defaults() {
    let request: Request = serde_json::from_str(r#"{"method":"wall.list"}"#).unwrap();
    assert_eq!(request.method, "wall.list");
    assert_eq!(request.id, 0);
    assert!(request.params.is_null());
}

#[test]
fn request_field_order() {
    let typed = Request { method: "wall.apply".into(), params: json!({"output": "DP-1"}), id: 7 };
    let wire = serde_json::to_string(&typed).unwrap();
    assert_eq!(wire, r#"{"method":"wall.apply","params":{"output":"DP-1"},"id":7}"#);

    let sorted = serde_json::to_string(
        &json!({ "method": "wall.apply", "params": {"output": "DP-1"}, "id": 7 }),
    )
    .unwrap();
    assert_eq!(sorted, r#"{"id":7,"method":"wall.apply","params":{"output":"DP-1"}}"#);

    let from_typed: Request = serde_json::from_str(&wire).unwrap();
    let from_sorted: Request = serde_json::from_str(&sorted).unwrap();
    assert_eq!(from_typed.method, from_sorted.method);
    assert_eq!(from_typed.params, from_sorted.params);
    assert_eq!(from_typed.id, from_sorted.id);
}

#[test]
fn response_event_untagged() {
    let response = serde_json::to_string(&Response::ok(1, json!(null))).unwrap();
    match serde_json::from_str::<ServerMessage>(&response).unwrap() {
        ServerMessage::Response(response) => assert_eq!(response.id, 1),
        ServerMessage::Event(_) => panic!("expected response"),
    }

    let event =
        serde_json::to_string(&Event { event: "skwd.wall.applied".into(), data: json!({}) })
            .unwrap();
    assert!(matches!(
        serde_json::from_str::<ServerMessage>(&event).unwrap(),
        ServerMessage::Event(_)
    ));

    let error = serde_json::to_string(&Response::err(7, -32601, "no such method")).unwrap();
    assert!(!error.contains("\"result\""));
    assert!(matches!(
        serde_json::from_str::<ServerMessage>(&error).unwrap(),
        ServerMessage::Response(Response { error: Some(_), .. })
    ));
}

#[test]
fn error_redacts_credentials() {
    let response = Response::err(
        8,
        -1,
        "request failed: https://wallhaven.cc/api/v1/search?apikey=REAL_SECRET&q=forest",
    );
    let message = response.error.unwrap().message;
    assert!(!message.contains("REAL_SECRET"));
    assert!(message.contains("apikey=[REDACTED]"));
    assert!(message.contains("q=forest"));
}
