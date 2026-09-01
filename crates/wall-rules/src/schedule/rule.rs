use std::cmp::Reverse;

use super::condition::{Clause, cmp_ord};
use super::date::date_in_range;
use super::expression::Expression;
use super::time::{At, fire_minute};

pub struct Rule {
    pub priority: i64,
    pub name: String,
    pub condition: Expression,
    pub set: String,
    pub mode: Option<String>,
}

#[derive(Debug)]
pub struct Now {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub wday: u32,
    pub minute: u32,
    pub sunrise: u32,
    pub sunset: u32,
    pub weather: Vec<String>,
    pub on_battery: Option<bool>,
    pub battery_percent: Option<u8>,
    pub outputs: Vec<String>,
}

fn window_active(from: At, until: At, minute: u32, sunrise: u32, sunset: u32) -> bool {
    let start = fire_minute(from, sunrise, sunset);
    let end = fire_minute(until, sunrise, sunset);
    if start <= end { minute >= start && minute < end } else { minute >= start || minute < end }
}

fn clause_matches(clause: &Clause, now: &Now) -> bool {
    match clause {
        Clause::TimeWindow(from, until) => {
            window_active(*from, *until, now.minute, now.sunrise, now.sunset)
        }
        Clause::Time(comparison, at) => {
            cmp_ord(*comparison, now.minute, fire_minute(*at, now.sunrise, now.sunset))
        }
        Clause::Weekday(days) => days.contains(&now.wday),
        Clause::Date(range) => date_in_range(range, now.year, now.month, now.day),
        Clause::Year(comparison, year) => cmp_ord(*comparison, i64::from(now.year), *year),
        Clause::Weather(set) => now.weather.iter().any(|tag| set.contains(tag)),
        Clause::Power(on_battery) => now.on_battery == Some(*on_battery),
        Clause::Battery(comparison, percent) => {
            now.battery_percent.is_some_and(|current| cmp_ord(*comparison, current, *percent))
        }
        Clause::Output(name) => now.outputs.iter().any(|output| output == name),
        Clause::OutputCount(comparison, count) => {
            cmp_ord(*comparison, now.outputs.len() as u32, *count)
        }
        Clause::Never => false,
    }
}

fn expression_matches(expression: &Expression, now: &Now) -> bool {
    match expression {
        Expression::Clause(clause) => clause_matches(clause, now),
        Expression::All(children) => children.iter().all(|child| expression_matches(child, now)),
        Expression::Any(children) => children.iter().any(|child| expression_matches(child, now)),
        Expression::Not(child) => !expression_matches(child, now),
    }
}

fn predicate_count(expression: &Expression) -> usize {
    match expression {
        Expression::Clause(_) => 1,
        Expression::All(children) | Expression::Any(children) => {
            children.iter().map(predicate_count).sum()
        }
        Expression::Not(child) => predicate_count(child),
    }
}

fn any_clause(expression: &Expression, predicate: fn(&Clause) -> bool) -> bool {
    match expression {
        Expression::Clause(clause) => predicate(clause),
        Expression::All(children) | Expression::Any(children) => {
            children.iter().any(|child| any_clause(child, predicate))
        }
        Expression::Not(child) => any_clause(child, predicate),
    }
}

fn visit_clauses(expression: &Expression, visit: &mut impl FnMut(&Clause)) {
    match expression {
        Expression::Clause(clause) => visit(clause),
        Expression::All(children) | Expression::Any(children) => {
            for child in children {
                visit_clauses(child, visit);
            }
        }
        Expression::Not(child) => visit_clauses(child, visit),
    }
}

fn rule_active(rule: &Rule, now: &Now) -> bool {
    expression_matches(&rule.condition, now)
}

pub fn winner(rules: &[Rule], now: &Now) -> Option<usize> {
    rules
        .iter()
        .enumerate()
        .filter(|(_, rule)| rule_active(rule, now))
        .min_by_key(|(index, rule)| {
            (rule.priority, Reverse(predicate_count(&rule.condition)), *index)
        })
        .map(|(index, _)| index)
}

pub fn uses_weather(rules: &[Rule]) -> bool {
    rules
        .iter()
        .any(|rule| any_clause(&rule.condition, |clause| matches!(clause, Clause::Weather(_))))
}

pub fn uses_power(rules: &[Rule]) -> bool {
    rules.iter().any(|rule| {
        any_clause(&rule.condition, |clause| {
            matches!(clause, Clause::Power(_) | Clause::Battery(..))
        })
    })
}

pub fn uses_outputs(rules: &[Rule]) -> bool {
    rules.iter().any(|rule| {
        any_clause(&rule.condition, |clause| {
            matches!(clause, Clause::Output(_) | Clause::OutputCount(..))
        })
    })
}

pub fn next_boundary_wait(rules: &[Rule], now_minute: u32, sunrise: u32, sunset: u32) -> u32 {
    let mut minutes: Vec<u32> = vec![0];
    for rule in rules {
        visit_clauses(&rule.condition, &mut |clause| match clause {
            Clause::TimeWindow(from, until) => {
                minutes.push(fire_minute(*from, sunrise, sunset));
                minutes.push(fire_minute(*until, sunrise, sunset));
            }
            Clause::Time(_, at) => minutes.push(fire_minute(*at, sunrise, sunset)),
            _ => {}
        });
    }
    let time_wait = minutes
        .iter()
        .map(
            |&minute| {
                if minute > now_minute { minute - now_minute } else { minute + 1440 - now_minute }
            },
        )
        .filter(|&wait| wait > 0)
        .min()
        .unwrap_or(1440);
    if uses_weather(rules) { time_wait.min(60) } else { time_wait }
}

#[cfg(test)]
#[path = "rule_tests.rs"]
mod tests;
