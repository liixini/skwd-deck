use serde_json::json;
use skwd_e2e::{Checks, Sandbox, Walld, ffmpeg_still};
use std::time::Duration;

const SUNDAY_2100_UTC: i64 = 1_767_560_400;

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn schedule_frozen_clock() {
    let mut sandbox = Sandbox::new("schedule");
    sandbox.set_env("SKWD_FAKE_TIME", &SUNDAY_2100_UTC.to_string());
    sandbox.set_env("TZ", "UTC");
    let lib = sandbox.library();

    let mut checks = Checks::default();
    let images_ok =
        [("sunday-night.png", "red"), ("fallback.png", "blue"), ("weekday.png", "green")]
            .iter()
            .all(|(name, color)| {
                ffmpeg_still(&lib.join(name), &format!("color=c={color}:s=64x36"))
            });
    checks.check("fixture images generated", images_ok, String::new);
    assert!(images_ok, "ffmpeg fixtures failed");

    let lib_str = lib.to_string_lossy().into_owned();
    sandbox.write_config(&json!({
        "paths": { "wallpaper": lib_str, "videoWallpaper": lib_str },
        "pickOnlyMode": true,
        "restoreOnStartup": false,
        "general": { "randomInterval": 0, "randomRotate": false },
        "effects": { "autoRecolor": false, "autoTheme": "" },
        "schedule": {
            "enabled": true,
            "migrated": true,
            "rules": [
                { "name": "paused-rule", "enabled": false, "priority": 5, "condition": {
                    "version": 2,
                    "root": {"kind": "group", "operator": "all", "children": []}
                }, "set": "fallback.png" },
                { "name": "sunday-night", "priority": 10, "condition": {
                    "version": 2,
                    "root": {"kind": "group", "operator": "all", "children": [
                        {"kind": "predicate", "value": "weekday:sun"},
                        {"kind": "group", "operator": "any", "children": [
                            {"kind": "predicate", "value": "time:>=sunset-30"},
                            {"kind": "predicate", "value": "weather:stormy"}
                        ]}
                    ]}
                }, "set": "sunday-night.png" },
                { "name": "weekday", "priority": 20, "condition": {
                    "version": 1,
                    "root": {"kind": "group", "operator": "all", "children": [
                        {"kind": "predicate", "value": "weekday:mon,tue,wed,thu,fri"}
                    ]}
                }, "set": "weekday.png" },
                { "name": "fallback", "priority": 90, "condition": {
                    "version": 1,
                    "root": {"kind": "group", "operator": "all", "children": []}
                }, "set": "fallback.png" },
            ],
        },
    }));

    let walld = Walld::start(&sandbox);
    checks.check("sandbox walld is up under the frozen clock", walld.responsive(), String::new);

    let fired = walld.wait_log("scheduled apply fired", Duration::from_secs(90));
    let sched_lines: String = walld
        .log_lines("scheduled apply fired")
        .into_iter()
        .chain(walld.log_lines("schedule: rule"))
        .collect::<Vec<_>>()
        .join("\n");
    let tail = |text: &str| {
        text.chars().rev().take(300).collect::<String>().chars().rev().collect::<String>()
    };

    checks.check("a schedule rule fired at the frozen Sunday 21:00", fired, || {
        format!("log: {}", tail(&sched_lines))
    });
    checks.check(
        "the sunday-night rule beat the fallback on priority",
        sched_lines.contains("set=sunday-night.png") && !sched_lines.contains("set=fallback.png"),
        || tail(&sched_lines),
    );
    checks.check(
        "the disabled higher-priority rule did not participate",
        !walld.log_lines("schedule: rule").iter().any(|line| line.contains("paused-rule")),
        || format!("{:?}", walld.log_lines("schedule: rule")),
    );
    checks.check(
        "the weekday rule did not fire on a sunday",
        !sched_lines.contains("set=weekday.png"),
        || tail(&sched_lines),
    );
    checks.check(
        "the engine logged the winning rule by name",
        walld.log_lines("schedule: rule").iter().any(|line| line.contains("sunday-night")),
        || format!("{:?}", walld.log_lines("schedule: rule")),
    );

    if checks.failed() {
        sandbox.mark_failed();
    }
    checks.finish();
}
