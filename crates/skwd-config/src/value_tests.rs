use super::*;
use serde_json::json;

#[test]
fn get_path_traverse() {
    let value = json!({"a": {"b": [{"c": 7}, {"c": 9}]}});
    assert_eq!(get(&value, "a.b.1.c"), Some(&json!(9)));
    assert_eq!(get(&value, "a.b.2.c"), None);
    assert_eq!(get(&value, "a.missing"), None);
}

#[test]
fn num_at_coerces_strings() {
    let value = json!({"a": 3.5, "b": "4.5", "c": " 6 ", "d": "x"});
    assert_eq!(num_at(&value, "a", 0.0), 3.5);
    assert_eq!(num_at(&value, "b", 0.0), 4.5);
    assert_eq!(num_at(&value, "c", 0.0), 6.0);
    assert_eq!(num_at(&value, "d", 1.0), 1.0);
}

#[test]
fn boolean_defaults() {
    let value = json!({"on": true, "off": false});
    assert!(bool_true_unless_false(&value, "missing"));
    assert!(!bool_true_unless_false(&value, "off"));
    assert!(!bool_false_unless_true(&value, "missing"));
    assert!(bool_false_unless_true(&value, "on"));
}

#[test]
fn str_ref_strict() {
    let value = json!({"a": {"b": "deep"}, "s": "top", "n": 7});
    assert_eq!(str_ref(&value, "s", "d"), "top");
    assert_eq!(str_ref(&value, "a.b", "d"), "deep");
    assert_eq!(str_ref(&value, "missing", "d"), "d");
    assert_eq!(str_ref(&value, "n", "d"), "d");
}

#[test]
fn typed_refs_no_coercion() {
    let value = json!({"i": -3, "u": 7, "f": 1.5, "b": true, "a": [1, 2], "s": "9"});
    assert_eq!(i64_ref(&value, "i", 0), -3);
    assert_eq!(i64_ref(&value, "s", 5), 5);
    assert_eq!(u64_ref(&value, "u", 0), 7);
    assert_eq!(u64_ref(&value, "i", 9), 9);
    assert_eq!(f64_ref(&value, "f", 0.0), 1.5);
    assert_eq!(arr_ref(&value, "a").len(), 2);
    assert!(arr_ref(&value, "missing").is_empty());
}
