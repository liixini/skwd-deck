#![cfg(test)]

use super::*;
use wall_rules::schedule::parse_date;

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

#[test]
fn parse_rule_requires_set() {
    let raw = serde_json::json!({
        "priority": 10, "name": "Cozy", "set": "video:fire.mp4", "mode": "dark",
        "condition": {"version": 1, "root": {
            "kind": "group", "operator": "all", "children": [
                {"kind": "predicate", "value": "weekday:sun"},
                {"kind": "predicate", "value": "weather:cloudy"},
                {"kind": "predicate", "value": "time:>=20:00"}
            ]
        }}
    });
    let rule = parse_rule(&raw).expect("rule with a set");
    assert_eq!(rule.priority, 10);
    assert_eq!(rule.name, "Cozy");
    assert_eq!(rule.set, "video:fire.mp4");
    assert_eq!(rule.mode.as_deref(), Some("dark"));
    assert!(matches!(rule.condition, Expression::All(ref conditions) if conditions.len() == 3));

    assert!(parse_rule(&serde_json::json!({ "condition": {} })).is_none());
    assert!(parse_rule(&serde_json::json!({ "set": "" })).is_none());
    assert!(parse_rule(&serde_json::json!({ "set": "random", "enabled": false })).is_none());
    let dflt = parse_rule(&serde_json::json!({ "set": "random" })).expect("set-only rule");
    assert_eq!(dflt.priority, 50);
    assert!(matches!(dflt.condition, Expression::Clause(Clause::Never)));
}

#[test]
fn catch_up_respects_user() {
    assert!(should_catch_up(None));
    assert!(should_catch_up(Some(ApplySource::Restore)));
    assert!(should_catch_up(Some(ApplySource::Schedule)));
    assert!(should_catch_up(Some(ApplySource::Rotation)));
    assert!(should_catch_up(Some(ApplySource::Hotplug)));
    assert!(!should_catch_up(Some(ApplySource::User)));
    assert!(!should_catch_up(Some(ApplySource::UserOverride)));
    assert!(!should_catch_up(Some(ApplySource::Random)));
    assert!(!should_catch_up(Some(ApplySource::Playlist)));
    assert!(!should_catch_up(Some(ApplySource::Replay)));
}

#[test]
fn utc_to_local_wraps() {
    assert_eq!(utc_to_local_min(60.0, 3600), 120);
    assert_eq!(utc_to_local_min(30.0, -3600), 1410);
}

#[test]
fn fake_time_env() {
    unsafe { std::env::set_var("SKWD_FAKE_TIME", "1735689600") };
    let first = local_now();
    let second = local_now();
    unsafe { std::env::remove_var("SKWD_FAKE_TIME") };
    assert_eq!(first.year, 2025);
    assert_eq!((first.min, first.doy, first.wday), (second.min, second.doy, second.wday));
    assert!(local_now().year >= 2026);
}

#[tokio::test]
async fn preset_wake_consumed() {
    use crate::infrastructure::wake::wake_or_timeout;

    reload();
    let t0 = std::time::Instant::now();
    assert!(!wake_or_timeout(&WAKE, Duration::from_secs(5)).await);
    assert!(t0.elapsed() < Duration::from_millis(500));
    let idle = tokio::sync::Notify::new();
    assert!(wake_or_timeout(&idle, Duration::from_millis(10)).await);
}

#[tokio::test(start_paused = true)]
async fn deadline_elapses_without_wake() {
    use crate::infrastructure::wake::wake_or_timeout;

    let wake = tokio::sync::Notify::new();
    let t0 = std::time::Instant::now();
    assert!(wake_or_timeout(&wake, Duration::from_secs(MAX_SCHEDULE_WAIT_SECS)).await);
    assert!(t0.elapsed() < Duration::from_secs(1));
}

#[tokio::test(start_paused = true)]
async fn wake_preempts_deadline() {
    use crate::infrastructure::wake::wake_or_timeout;

    let wake = Arc::new(tokio::sync::Notify::new());
    let notifier = Arc::clone(&wake);
    let pending = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(60)).await;
        notifier.notify_one();
    });
    assert!(!wake_or_timeout(&wake, Duration::from_secs(MAX_SCHEDULE_WAIT_SECS)).await);
    pending.await.unwrap();
}

use skwd_wall_core::xorshift64 as xs;

fn fuzz_string(seed: &mut u64) -> String {
    const POOL: &[&str] = &[
        "time",
        "weekday",
        "date",
        "year",
        "weather",
        "power",
        "battery",
        "output",
        "outputs",
        ":",
        ",",
        "..",
        ">=",
        "<=",
        ">",
        "<",
        "=",
        "-",
        " ",
        "20:00",
        "sun",
        "99:99",
        "0",
        "13-40",
        "2026",
        "sunrise",
        "sunset",
        "battery",
        "external",
        "DP-3",
        "🌊",
        "é",
        "\u{0}",
        "999999999999999999999",
        "-1",
        "1.5",
        "\t",
        "never:",
        "::",
    ];
    let len = (xs(seed) % 16) as usize;
    (0..len).map(|_| POOL[(xs(seed) % POOL.len() as u64) as usize]).collect()
}

#[test]
fn parser_fuzz() {
    let mut seed = 0x5eed_0003u64;
    let now = moment(2026, 1, 4, 0, 1260, &[]);
    for _ in 0..8000 {
        let text = fuzz_string(&mut seed);
        let rule = Rule {
            priority: 50,
            name: String::new(),
            condition: parse_expression(&serde_json::json!({
                "version": 1,
                "root": {"kind": "group", "operator": "all", "children": [
                    {"kind": "predicate", "value": text}
                ]}
            })),
            set: "x".into(),
            mode: None,
        };
        let _ = winner(std::slice::from_ref(&rule), &now);
        let _ = parse_date(&text);
        let _ = parse_at(&text);
        let _ = parse_rule(&serde_json::json!({
            "set": "x",
            "condition": {"version": 1, "root": {
                "kind": "predicate", "value": text
            }},
            "priority": 1
        }));
    }
    assert!(matches!(
        parse_expression(&serde_json::json!({
            "version": 1,
            "root": {"kind": "group", "operator": "all", "children": [
                {"kind": "predicate", "value": "garbage"}
            ]}
        })),
        Expression::Clause(Clause::Never)
    ));
}
