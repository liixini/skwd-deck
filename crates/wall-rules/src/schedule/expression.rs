use serde_json::Value;

use super::condition::{Clause, parse_clause};

pub enum Expression {
    Clause(Clause),
    All(Vec<Expression>),
    Any(Vec<Expression>),
    Not(Box<Expression>),
}

const MAX_DEPTH: usize = 32;
const MAX_NODES: usize = 256;

fn maybe_negate(expression: Expression, node: &Value) -> Option<Expression> {
    match node.get("negated") {
        None | Some(Value::Bool(false)) => Some(expression),
        Some(Value::Bool(true)) => Some(Expression::Not(Box::new(expression))),
        Some(_) => None,
    }
}

fn parse_node(
    node: &Value,
    version: u64,
    depth: usize,
    remaining: &mut usize,
) -> Option<Expression> {
    if depth > MAX_DEPTH || *remaining == 0 {
        return None;
    }
    *remaining -= 1;
    match node.get("kind").and_then(Value::as_str) {
        Some("predicate") => {
            let clause = parse_clause(node.get("value")?.as_str()?, version);
            if matches!(clause, Clause::Never) {
                return None;
            }
            maybe_negate(Expression::Clause(clause), node)
        }
        Some("group") => {
            let children = node
                .get("children")?
                .as_array()?
                .iter()
                .map(|child| parse_node(child, version, depth + 1, remaining))
                .collect::<Option<Vec<_>>>()?;
            let expression = match node.get("operator").and_then(Value::as_str) {
                Some("all") => Expression::All(children),
                Some("any") => Expression::Any(children),
                _ => return None,
            };
            maybe_negate(expression, node)
        }
        _ => None,
    }
}

pub fn parse_expression(value: &Value) -> Expression {
    let Some(version @ (1 | 2)) = value.get("version").and_then(Value::as_u64) else {
        return Expression::Clause(Clause::Never);
    };
    let Some(root) = value.get("root") else {
        return Expression::Clause(Clause::Never);
    };
    if root.get("kind").and_then(Value::as_str) != Some("group") {
        return Expression::Clause(Clause::Never);
    }
    let mut remaining = MAX_NODES;
    parse_node(root, version, 0, &mut remaining).unwrap_or(Expression::Clause(Clause::Never))
}

#[cfg(test)]
#[path = "expression_tests.rs"]
mod tests;
