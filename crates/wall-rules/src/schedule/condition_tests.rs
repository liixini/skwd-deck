use super::*;
use proptest::prelude::*;

#[test]
fn cmp_operators() {
    assert!(cmp_ord(Cmp::Ge, 5, 5) && cmp_ord(Cmp::Ge, 6, 5) && !cmp_ord(Cmp::Ge, 4, 5));
    assert!(cmp_ord(Cmp::Le, 5, 5) && cmp_ord(Cmp::Le, 4, 5) && !cmp_ord(Cmp::Le, 6, 5));
    assert!(cmp_ord(Cmp::Gt, 6, 5) && !cmp_ord(Cmp::Gt, 5, 5) && !cmp_ord(Cmp::Gt, 4, 5));
    assert!(cmp_ord(Cmp::Lt, 4, 5) && !cmp_ord(Cmp::Lt, 5, 5) && !cmp_ord(Cmp::Lt, 6, 5));
    assert!(cmp_ord(Cmp::Eq, 5, 5) && !cmp_ord(Cmp::Eq, 4, 5) && !cmp_ord(Cmp::Eq, 6, 5));
}

#[test]
fn weekday_parsing() {
    assert_eq!(weekday_num("sun"), Some(0));
    assert_eq!(weekday_num("Sunday"), Some(0));
    assert_eq!(weekday_num("sat"), Some(6));
    assert_eq!(weekday_num("6"), Some(6));
    assert_eq!(weekday_num("7"), None);
    assert_eq!(weekday_num("nope"), None);
}

#[test]
fn malformed_conditions_no_panic() {
    for condition in ["", "weather:", "time:99:99", "zzz:qqq", "date:-", "nope"] {
        let _ = crate::schedule::parse_expression(&serde_json::json!({
            "version": 1,
            "root": {"kind": "group", "operator": "all", "children": [
                {"kind": "predicate", "value": condition}
            ]}
        }));
    }
}

proptest! {
    #[test]
    fn random_strings_no_panic(input in ".{0,80}") {
        let _ = crate::schedule::parse_expression(&serde_json::json!({
            "version": 1,
            "root": {"kind": "group", "operator": "all", "children": [
                {"kind": "predicate", "value": input}
            ]}
        }));
    }
}
