use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use skwd_wall_core::WallState;
use skwd_wall_core::backend::wallpaper::WallpaperApplication;
use tokio::sync::Notify;

use crate::backend::events::EventPublisher;
use crate::backend::history::{ApplySource, HistoryRepository};
use crate::composition::runtime::rotation::rotate_once;
use crate::infrastructure::stats::Stats;
use wall_rules::schedule::{
    At, Clause, Expression, Now, Rule, next_boundary_wait, parse_at, parse_expression,
    sun_times_utc_min, uses_outputs, uses_power, uses_weather, winner,
};

const MAX_SCHEDULE_WAIT_SECS: u64 = 6 * 3600;

pub fn should_catch_up(last: Option<ApplySource>) -> bool {
    !matches!(
        last,
        Some(
            ApplySource::User
                | ApplySource::UserOverride
                | ApplySource::Random
                | ApplySource::Playlist
                | ApplySource::Replay,
        )
    )
}

fn utc_to_local_min(utc_min: f64, gmtoff_secs: i64) -> u32 {
    let local = utc_min + gmtoff_secs as f64 / 60.0;
    (local.round() as i64).rem_euclid(1440) as u32
}

struct Local {
    min: u32,
    doy: u32,
    wday: u32,
    gmtoff: i64,
    year: i32,
    month: u32,
    day: u32,
}

fn epoch_secs() -> i64 {
    if let Some(fake) = std::env::var("SKWD_FAKE_TIME").ok().and_then(|val| val.parse::<i64>().ok())
    {
        return fake;
    }
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |dur| dur.as_secs() as i64)
}

fn local_now() -> Local {
    let secs = epoch_secs();
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        let tsecs = secs as libc::time_t;
        libc::localtime_r(&tsecs, &mut tm);
        Local {
            min: (tm.tm_hour * 60 + tm.tm_min) as u32,
            doy: (tm.tm_yday + 1) as u32,
            wday: tm.tm_wday as u32,
            gmtoff: tm.tm_gmtoff,
            year: tm.tm_year + 1900,
            month: (tm.tm_mon + 1) as u32,
            day: tm.tm_mday as u32,
        }
    }
}

use crate::infrastructure::wake::wake_or_timeout;

static WAKE: Notify = Notify::const_new();

pub fn reload() {
    WAKE.notify_one();
}

pub fn start_scheduler(ctx: crate::composition::context::Ctx) {
    tokio::spawn(scheduler_loop(ctx));
}

fn parse_rule(val: &Value) -> Option<Rule> {
    if val.get("enabled").and_then(Value::as_bool) == Some(false) {
        return None;
    }
    let set = val.get("set").and_then(Value::as_str).filter(|text| !text.is_empty())?.to_string();
    Some(Rule {
        priority: val.get("priority").and_then(Value::as_i64).unwrap_or(50),
        name: val.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
        condition: val.get("condition").map_or(Expression::Clause(Clause::Never), parse_expression),
        set,
        mode: val
            .get("mode")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .map(String::from),
    })
}

fn load_rules(cfg: &skwd_wall_core::config::Config) -> Vec<Rule> {
    let mut out = Vec::new();
    if !cfg.schedule_enabled() {
        return out;
    }
    if !cfg.schedule_migrated() {
        let solar = cfg.schedule_solar();
        let day_at = if solar {
            At::Sunrise(0)
        } else {
            parse_at(&cfg.schedule_str("dayTime")).unwrap_or(At::Clock(7 * 60))
        };
        let night_at = if solar {
            At::Sunset(0)
        } else {
            parse_at(&cfg.schedule_str("nightTime")).unwrap_or(At::Clock(20 * 60))
        };
        let opt = |text: String| (!text.is_empty()).then_some(text);
        let non_empty =
            |text: String, def: &str| if text.is_empty() { def.to_string() } else { text };
        out.push(Rule {
            priority: 100,
            name: "Day".into(),
            condition: Expression::Clause(Clause::TimeWindow(day_at, night_at)),
            set: non_empty(cfg.schedule_str("daySet"), "random"),
            mode: opt(cfg.schedule_str("dayMode")),
        });
        out.push(Rule {
            priority: 100,
            name: "Night".into(),
            condition: Expression::Clause(Clause::TimeWindow(night_at, day_at)),
            set: non_empty(cfg.schedule_str("nightSet"), "random"),
            mode: opt(cfg.schedule_str("nightMode")),
        });
    }
    for val in cfg.schedule_rules() {
        if let Some(rule) = parse_rule(&val) {
            out.push(rule);
        }
    }
    out
}

async fn scheduler_loop(ctx: crate::composition::context::Ctx) {
    let mut first = true;
    let mut last_applied: Option<(String, Option<String>)> = None;
    loop {
        let rules_state = Arc::clone(&ctx.state);
        let Ok(rules) = tokio::task::spawn_blocking(move || {
            rules_state.reload_config();
            load_rules(&rules_state.config())
        })
        .await
        else {
            break;
        };
        if rules.is_empty() {
            wake_or_timeout(&WAKE, crate::composition::bootstrap::IDLE_RECHECK).await;
            continue;
        }
        let loc = local_now();
        let (sunrise, sunset) = sun_window(&ctx.state, loc.doy, loc.gmtoff);
        let weather = if uses_weather(&rules) {
            let (locale, lat, lon) = {
                let cfg = ctx.state.config();
                (cfg.locale(), cfg.latitude(), cfg.longitude())
            };
            tokio::task::spawn_blocking(move || {
                crate::infrastructure::weather::current(&locale, lat, lon)
            })
            .await
            .unwrap_or_default()
        } else {
            Vec::new()
        };
        let power_needed = uses_power(&rules);
        let outputs_needed = uses_outputs(&rules);
        let (on_battery, battery_percent, outputs) = tokio::task::spawn_blocking(move || {
            let on_battery = power_needed
                .then(skwd_config::power_source_state)
                .and_then(skwd_config::PowerSourceState::on_battery);
            let battery_percent = power_needed.then(skwd_config::battery_percent).flatten();
            let outputs =
                if outputs_needed { skwd_wall_core::outputs::names() } else { Vec::new() };
            (on_battery, battery_percent, outputs)
        })
        .await
        .unwrap_or_default();
        let now = Now {
            year: loc.year,
            month: loc.month,
            day: loc.day,
            wday: loc.wday,
            minute: loc.min,
            sunrise,
            sunset,
            weather,
            on_battery,
            battery_percent,
            outputs,
        };
        if let Some(idx) = winner(&rules, &now) {
            let rule = &rules[idx];
            let key = (rule.set.clone(), rule.mode.clone());
            let skip_startup = std::mem::take(&mut first)
                && (!ctx.state.config().schedule_apply_on_start()
                    || !should_catch_up(crate::composition::history::last_apply_source()));
            if skip_startup {
                last_applied = Some(key);
            } else if last_applied.as_ref() != Some(&key) {
                log::info!("schedule: rule '{}' (priority {}) wins", rule.name, rule.priority);
                let task = ctx.clone();
                let set = rule.set.clone();
                let mode = rule.mode.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    fire(
                        &task.state,
                        task.wallpaper.as_ref(),
                        task.history.as_ref(),
                        task.events.as_ref(),
                        &task.stats,
                        &set,
                        mode.as_deref(),
                    );
                })
                .await;
                last_applied = Some(key);
            }
        }
        let wait_min = next_boundary_wait(&rules, loc.min, sunrise, sunset);
        let capped = (wait_min as u64 * 60).clamp(1, MAX_SCHEDULE_WAIT_SECS);
        wake_or_timeout(&WAKE, Duration::from_secs(capped)).await;
    }
}

fn sun_window(state: &WallState, doy: u32, gmtoff: i64) -> (u32, u32) {
    let (lat, lon) = {
        let cfg = state.config();
        (cfg.latitude(), cfg.longitude())
    };
    match sun_times_utc_min(doy, lat, lon) {
        Some((sr, ss)) => (utc_to_local_min(sr, gmtoff), utc_to_local_min(ss, gmtoff)),
        None => (6 * 60, 18 * 60),
    }
}

fn fire(
    state: &Arc<WallState>,
    application: &dyn WallpaperApplication,
    history: &dyn HistoryRepository,
    publisher: &dyn EventPublisher,
    stats: &Arc<Stats>,
    set: &str,
    mode: Option<&str>,
) {
    stats.set_task("scheduled");
    if let Some(id) = set.strip_prefix("playlist:").and_then(|rest| rest.trim().parse::<i64>().ok())
    {
        let _ = state.with_db(|conn| skwd_wall_core::db::playlist_assign_set(conn, "*", Some(id)));
        crate::composition::runtime::playlist::reload();
        stats.set_task("idle");
        log::info!("scheduled playlist {id} assigned to all outputs");
        return;
    }
    let _ = state.with_db(|conn| skwd_wall_core::db::playlist_assign_set(conn, "*", None));
    crate::composition::runtime::playlist::reload();
    let applied = if set == "random" {
        let (types, fav) = {
            let cfg = state.config();
            (cfg.random_types(), cfg.random_favourites_only())
        };
        let refs: Vec<&str> = types.iter().map(String::as_str).collect();
        rotate_once(
            state,
            application,
            history,
            publisher,
            stats,
            &refs,
            fav,
            None,
            ApplySource::Schedule,
        )
        .is_some()
    } else {
        crate::composition::apply::apply_by_key(
            state,
            application,
            history,
            publisher,
            stats,
            set,
            "*",
            true,
            ApplySource::Schedule,
        )
    };
    stats.set_task("idle");
    if applied {
        if let (Some(mode), Some(src)) = (mode, state.theme().source()) {
            let cfg = state.config().clone();
            skwd_wall_core::matugen::run_with(&cfg, &src, None, Some(mode), None);
        }
        log::info!("scheduled apply fired: set={set} mode={mode:?}");
    }
}

mod tests;
