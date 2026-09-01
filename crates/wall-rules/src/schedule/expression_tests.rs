use serde_json::json;

use super::{Expression, parse_expression};
use crate::schedule::Clause;

#[test]
fn nested_groups_and_negation() {
    let expression = parse_expression(&json!({
        "version": 1,
        "root": {
            "kind": "group",
            "operator": "all",
            "negated": false,
            "children": [
                {"kind": "predicate", "value": "weekday:sat,sun", "negated": false},
                {"kind": "group", "operator": "any", "negated": true, "children": [
                    {"kind": "predicate", "value": "weather:rainy", "negated": false},
                    {"kind": "predicate", "value": "time:>=sunset", "negated": true}
                ]}
            ]
        }
    }));
    let Expression::All(root) = expression else {
        panic!("root must be all");
    };
    assert_eq!(root.len(), 2);
    assert!(matches!(root[0], Expression::Clause(Clause::Weekday(_))));
    assert!(matches!(root[1], Expression::Not(_)));
}

#[test]
fn unknown_versions_fail_closed() {
    for value in [
        json!({}),
        json!({"version": 3, "root": {"kind": "group", "operator": "all", "children": []}}),
        json!({"version": 1, "root": {"kind": "group", "operator": "xor", "children": []}}),
        json!({"version": 1, "root": {"kind": "predicate", "value": "unknown:value"}}),
    ] {
        assert!(matches!(parse_expression(&value), Expression::Clause(Clause::Never)));
    }
}

#[test]
fn v2_context_predicates() {
    let root = json!({"kind": "group", "operator": "all", "children": [
        {"kind": "predicate", "value": "time:>=sunset-30"},
        {"kind": "predicate", "value": "power:battery"},
        {"kind": "predicate", "value": "battery:<=30"},
        {"kind": "predicate", "value": "output:DP-3"},
        {"kind": "predicate", "value": "outputs:>=2"}
    ]});
    assert!(matches!(
        parse_expression(&json!({"version": 2, "root": root.clone()})),
        Expression::All(ref children) if children.len() == 5
    ));
    assert!(matches!(
        parse_expression(&json!({"version": 1, "root": root})),
        Expression::Clause(Clause::Never)
    ));
}

#[test]
fn malformed_branch_fails_tree() {
    let expression = parse_expression(&json!({
        "version": 1,
        "root": {"kind": "group", "operator": "any", "children": [
            {"kind": "group", "operator": "all", "children": []},
            {"kind": "predicate", "value": "unknown:value"}
        ]}
    }));
    assert!(matches!(expression, Expression::Clause(Clause::Never)));
}

#[test]
fn depth_and_node_limits() {
    let mut root = json!({"kind": "group", "operator": "all", "children": []});
    for _ in 0..33 {
        root = json!({"kind": "group", "operator": "all", "children": [root]});
    }
    assert!(matches!(
        parse_expression(&json!({"version": 1, "root": root})),
        Expression::Clause(Clause::Never)
    ));

    let children =
        (0..256).map(|_| json!({"kind": "predicate", "value": "weekday:sun"})).collect::<Vec<_>>();
    assert!(matches!(
        parse_expression(&json!({"version": 1, "root": {
            "kind": "group", "operator": "all", "children": children
        }})),
        Expression::Clause(Clause::Never)
    ));
}
