#![cfg(test)]

use super::*;
use crate::db::open_in_memory;

#[test]
fn overrides_roundtrip_per_item() {
    let conn = open_in_memory().unwrap();
    assert!(we_properties(&conn, "123").is_empty());

    set_we_property(&conn, "123", "tint", Some(&Value::String("1 0 0".into()))).unwrap();
    set_we_property(&conn, "123", "zoom", Some(&serde_json::json!(2.5))).unwrap();
    set_we_property(&conn, "123", "glow", Some(&Value::Bool(true))).unwrap();
    set_we_property(&conn, "456", "zoom", Some(&serde_json::json!(0.5))).unwrap();

    let item = we_properties(&conn, "123");
    assert_eq!(item.get("tint").unwrap(), &Value::String("1 0 0".into()));
    assert_eq!(item.get("zoom").unwrap(), &serde_json::json!(2.5));
    assert_eq!(item.get("glow").unwrap(), &Value::Bool(true));
    assert_eq!(item.len(), 3);
    assert_eq!(we_properties(&conn, "456").len(), 1);

    set_we_property(&conn, "123", "zoom", Some(&serde_json::json!(3.0))).unwrap();
    assert_eq!(we_properties(&conn, "123").get("zoom").unwrap(), &serde_json::json!(3.0));

    set_we_property(&conn, "123", "zoom", None).unwrap();
    assert!(!we_properties(&conn, "123").contains_key("zoom"));

    clear_we_properties(&conn, "123").unwrap();
    assert!(we_properties(&conn, "123").is_empty());
    assert_eq!(we_properties(&conn, "456").len(), 1);
}

#[test]
fn property_name_bounds() {
    assert!(valid_property_name("zoom"));
    assert!(!valid_property_name(""));
    assert!(!valid_property_name("   "));
    assert!(!valid_property_name(&"n".repeat(MAX_WE_PROPERTY_NAME + 1)));
    assert!(valid_property_name(&"n".repeat(MAX_WE_PROPERTY_NAME)));
}
