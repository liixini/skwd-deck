use super::*;
use crate::schedule::parse_expression;
use proptest::prelude::*;
use serde_json::json;

fn rule(priority: i64, name: &str, when: &str) -> Rule {
    let children = when
        .split_whitespace()
        .map(|value| json!({"kind": "predicate", "value": value}))
        .collect::<Vec<_>>();
    Rule {
        priority,
        name: name.into(),
        condition: parse_expression(&json!({
            "version": 1,
            "root": {"kind": "group", "operator": "all", "children": children}
        })),
        set: name.into(),
        mode: None,
    }
}

fn moment(year: i32, month: u32, day: u32, wday: u32, minute: u32, weather: &[&str]) -> Now {
    Now {
        year,
        month,
        day,
        wday,
        minute,
        sunrise: 360,
        sunset: 1080,
        weather: weather.iter().map(|tag| (*tag).to_string()).collect(),
        on_battery: None,
        battery_percent: None,
        outputs: Vec::new(),
    }
}

fn winning_set<'a>(rules: &'a [Rule], now: &Now) -> Option<&'a str> {
    winner(rules, now).map(|index| rules[index].set.as_str())
}

#[test]
fn lower_priority_wins() {
    let rules = vec![
        rule(100, "default", ""),
        rule(20, "cozy", "weekday:sun weather:cloudy time:>=20:00 year:2026"),
        rule(10, "fireworks", "date:07-04"),
    ];
    assert_eq!(winning_set(&rules, &moment(2026, 3, 1, 0, 21 * 60, &["cloudy"])), Some("cozy"));
    assert_eq!(winning_set(&rules, &moment(2026, 3, 2, 1, 21 * 60, &["cloudy"])), Some("default"));
    assert_eq!(winning_set(&rules, &moment(2026, 7, 4, 6, 720, &[])), Some("fireworks"));
}

#[test]
fn nested_boolean_groups() {
    let condition = parse_expression(&json!({
        "version": 1,
        "root": {"kind": "group", "operator": "all", "children": [
            {"kind": "predicate", "value": "weekday:sat,sun"},
            {"kind": "group", "operator": "any", "children": [
                {"kind": "predicate", "value": "weather:rainy"},
                {"kind": "predicate", "value": "time:>=sunset"},
                {"kind": "predicate", "value": "weather:stormy", "negated": true}
            ]}
        ]}
    }));
    let rules = vec![Rule {
        priority: 10,
        name: "weekend".into(),
        condition,
        set: "weekend".into(),
        mode: None,
    }];
    assert_eq!(winning_set(&rules, &moment(2026, 8, 15, 6, 720, &["rainy"])), Some("weekend"));
    assert_eq!(winning_set(&rules, &moment(2026, 8, 15, 6, 1200, &["stormy"])), Some("weekend"));
    assert_eq!(winning_set(&rules, &moment(2026, 8, 15, 6, 720, &["stormy"])), None);
    assert_eq!(winning_set(&rules, &moment(2026, 8, 17, 1, 1200, &[])), None);
}

#[test]
fn not_wraps_group() {
    let rules = vec![Rule {
        priority: 10,
        name: "dry".into(),
        condition: parse_expression(&json!({
            "version": 1,
            "root": {"kind": "group", "operator": "all", "children": [
                {"kind": "group", "operator": "any", "negated": true, "children": [
                    {"kind": "predicate", "value": "weather:rainy"},
                    {"kind": "predicate", "value": "weather:stormy"}
                ]}
            ]}
        })),
        set: "dry".into(),
        mode: None,
    }];
    assert_eq!(winning_set(&rules, &moment(2026, 8, 15, 6, 720, &["clear"])), Some("dry"));
    assert_eq!(winning_set(&rules, &moment(2026, 8, 15, 6, 720, &["rainy"])), None);
}

#[test]
fn equal_priority_specificity() {
    let rules = vec![
        rule(20, "february", "date:02-01..02-28"),
        rule(20, "feb-evenings", "date:02-01..02-28 time:>=18:00"),
    ];
    assert_eq!(winning_set(&rules, &moment(2026, 2, 14, 3, 20 * 60, &[])), Some("feb-evenings"));
    assert_eq!(winning_set(&rules, &moment(2026, 2, 14, 3, 720, &[])), Some("february"));
}

#[test]
fn wrapping_windows_and_typos() {
    let rules =
        vec![rule(10, "day", "time:sunrise..sunset"), rule(10, "night", "time:sunset..sunrise")];
    assert_eq!(winning_set(&rules, &moment(2026, 6, 1, 1, 720, &[])), Some("day"));
    assert_eq!(winning_set(&rules, &moment(2026, 6, 1, 1, 1200, &[])), Some("night"));

    let malformed = vec![rule(10, "bad", "wether:cloudy")];
    assert_eq!(winning_set(&malformed, &moment(2026, 1, 1, 3, 720, &["cloudy"])), None);
}

#[test]
fn weather_caps_boundary_wait() {
    let day = vec![rule(10, "day", "time:06:00..18:00")];
    assert_eq!(next_boundary_wait(&day, 8 * 60, 360, 1080), 10 * 60);
    assert_eq!(next_boundary_wait(&day, 20 * 60, 360, 1080), 4 * 60);

    let weathered = vec![rule(10, "day", "time:06:00..18:00"), rule(5, "rain", "weather:rainy")];
    assert_eq!(next_boundary_wait(&weathered, 8 * 60, 360, 1080), 60);
    assert!(uses_weather(&weathered));
}

#[test]
fn date_year_weather_weekday() {
    let rules = vec![
        rule(100, "default", ""),
        rule(50, "vacation", "date:07-01..07-14"),
        rule(10, "future-rain", "year:>=2030 weather:rainy weekday:sat,sun"),
    ];
    assert_eq!(winning_set(&rules, &moment(2026, 7, 10, 5, 720, &[])), Some("vacation"));
    assert_eq!(winning_set(&rules, &moment(2031, 7, 12, 6, 720, &["rainy"])), Some("future-rain"));
    assert_eq!(winning_set(&rules, &moment(2031, 7, 12, 3, 720, &["rainy"])), Some("vacation"));
}

#[test]
fn v2_context_clauses() {
    let condition = parse_expression(&json!({
        "version": 2,
        "root": {"kind": "group", "operator": "all", "children": [
            {"kind": "predicate", "value": "power:battery"},
            {"kind": "predicate", "value": "battery:<=30"},
            {"kind": "predicate", "value": "output:DP-3"},
            {"kind": "predicate", "value": "outputs:>=2"}
        ]}
    }));
    let rules = vec![Rule {
        priority: 10,
        name: "portable".into(),
        condition,
        set: "portable".into(),
        mode: None,
    }];
    let mut now = moment(2026, 8, 24, 1, 720, &[]);
    now.on_battery = Some(true);
    now.battery_percent = Some(24);
    now.outputs = vec!["eDP-1".into(), "DP-3".into()];
    assert_eq!(winning_set(&rules, &now), Some("portable"));
    assert!(uses_power(&rules));
    assert!(uses_outputs(&rules));

    now.battery_percent = None;
    assert_eq!(winning_set(&rules, &now), None);
    now.battery_percent = Some(24);
    now.outputs.pop();
    assert_eq!(winning_set(&rules, &now), None);
}

fn arbitrary_condition() -> impl Strategy<Value = String> {
    let token = prop_oneof![
        Just("weekday:sun".to_string()),
        Just("weekday:mon,tue,wed,thu,fri".to_string()),
        Just("time:>=20:00".to_string()),
        Just("time:<08:00".to_string()),
        (0u32..24, 0u32..60).prop_map(|(hour, minute)| format!("time:>={hour:02}:{minute:02}")),
        Just("date:12-01..12-31".to_string()),
        Just("weather:rain".to_string()),
        "[a-z]{2,8}:[a-z0-9]{1,6}",
    ];
    prop::collection::vec(token, 0..3).prop_map(|tokens| tokens.join(" "))
}

fn arbitrary_now() -> impl Strategy<Value = Now> {
    let weather = prop::collection::vec(
        prop_oneof![Just("rain".to_string()), Just("clear".to_string()), Just("snow".to_string())],
        0..2,
    );
    (2000i32..2100, 1u32..=12, 1u32..=28, 0u32..=6, 0u32..1440, weather).prop_map(
        |(year, month, day, wday, minute, weather)| Now {
            year,
            month,
            day,
            wday,
            minute,
            sunrise: 360,
            sunset: 1080,
            weather,
            on_battery: None,
            battery_percent: None,
            outputs: Vec::new(),
        },
    )
}

fn build(specs: &[(i64, String)]) -> Vec<Rule> {
    specs
        .iter()
        .enumerate()
        .map(|(index, (priority, when))| rule(*priority, &format!("r{index}"), when))
        .collect()
}

fn sort_key(rule: &Rule, index: usize) -> (i64, Reverse<usize>, usize) {
    (rule.priority, Reverse(predicate_count(&rule.condition)), index)
}

proptest! {
    #[test]
    fn winner_lowest_key(
        specs in prop::collection::vec((0i64..50, arbitrary_condition()), 0..8),
        now in arbitrary_now(),
    ) {
        let rules = build(&specs);
        match winner(&rules, &now) {
            None => prop_assert!(rules.iter().all(|candidate| !rule_active(candidate, &now))),
            Some(won) => {
                prop_assert!(rule_active(&rules[won], &now));
                for (index, candidate) in rules.iter().enumerate() {
                    if rule_active(candidate, &now) {
                        prop_assert!(sort_key(&rules[won], won) <= sort_key(candidate, index));
                    }
                }
            }
        }
    }

    #[test]
    fn inactive_rule_preserves_winner(
        specs in prop::collection::vec((0i64..50, arbitrary_condition()), 1..6),
        extra in (0i64..50, arbitrary_condition()),
        now in arbitrary_now(),
    ) {
        let extra_rule = rule(extra.0, "extra", &extra.1);
        prop_assume!(!rule_active(&extra_rule, &now));
        let before_rules = build(&specs);
        let before = winner(&before_rules, &now).map(|index| before_rules[index].set.clone());
        let mut after_rules = build(&specs);
        after_rules.push(extra_rule);
        let after = winner(&after_rules, &now).map(|index| after_rules[index].set.clone());
        prop_assert_eq!(before, after);
    }

    #[test]
    fn malformed_never_beats_base(now in arbitrary_now(), priority in -100i64..100) {
        let rules = vec![
            rule(priority, "bad", "zzz:qqq nope:nope"),
            rule(100, "base", ""),
        ];
        prop_assert_eq!(winning_set(&rules, &now), Some("base"));
    }
}
