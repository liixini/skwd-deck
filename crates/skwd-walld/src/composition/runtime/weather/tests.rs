use super::*;
use crate::testenv::cache_item;

fn active() -> WeatherStatus {
    WeatherStatus {
        enabled: true,
        weather: vec!["clear".into()],
        category: Some("clear".into()),
        playlist_id: Some(7),
        member_keys: vec!["static:clear.png".into()],
        resolved_count: 1,
        status: WeatherState::Active,
        ..WeatherStatus::default()
    }
}

fn runtime(name: &str) -> Runtime {
    let path = std::env::temp_dir().join(format!("weather-{name}-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&path);
    Runtime::load(path)
}

#[test]
fn manual_pause_scope() {
    let mut runtime = runtime("scope");
    runtime.update(active());
    runtime.policy.paused.insert("DP-1".into());
    runtime.update(active());
    assert_eq!(runtime.status.status, WeatherState::Paused);
    assert!(runtime.status.resume_available);
    assert!(runtime.assignment("DP-1").is_none());
    assert!(runtime.assignment("DP-2").is_some());
    runtime.policy.paused.insert("*".into());
    assert!(runtime.assignment("DP-2").is_none());
}

#[test]
fn same_mapping_holds_pause() {
    let mut runtime = runtime("same");
    runtime.update(active());
    runtime.policy.paused.insert("DP-1".into());
    let mut next = active();
    next.weather.push("sunny".into());
    next.fetched_at = Some(55);
    runtime.update(next);
    assert!(runtime.assignment("DP-1").is_none());
}

#[test]
fn mapping_change_resumes() {
    let mut runtime = runtime("change");
    runtime.update(active());
    runtime.policy.paused.insert("*".into());
    let mut next = active();
    next.playlist_id = Some(8);
    runtime.update(next);
    assert_eq!(runtime.assignment("DP-1").unwrap().0, Some(8));
    assert!(runtime.policy.paused.is_empty());
}

#[test]
fn semantic_query_changes_pause() {
    let mut runtime = runtime("semantic-query");
    let mut first = active();
    first.source = WeatherSource::Semantic;
    first.playlist_id = None;
    first.query = Some("snow at night".into());
    runtime.update(first.clone());
    runtime.policy.paused.insert("DP-1".into());
    runtime.update(first.clone());
    assert!(runtime.assignment("DP-1").is_none());
    first.query = Some("snowy mountain".into());
    runtime.update(first);
    assert_eq!(runtime.assignment("DP-1").unwrap().0, None);
    assert!(runtime.policy.paused.is_empty());
}

#[test]
fn approved_query_keeps_pause() {
    let mut runtime = runtime("approved-query");
    let mut first = active();
    first.source = WeatherSource::Semantic;
    first.selection = WeatherSelection::Approved;
    first.playlist_id = None;
    first.query = Some("old sky".into());
    runtime.update(first.clone());
    runtime.policy.paused.insert("DP-1".into());
    first.query = Some("new sky".into());
    runtime.update(first);
    assert!(runtime.assignment("DP-1").is_none());
    assert!(runtime.status.resume_available);
}

#[test]
fn manual_pause_during_query() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(
        serde_json::json!({"weather":{"enabled":true,"source":"semantic","semantic":{"queries":{"clear":"old sky"}}}}),
    );
    let (state, _, _) = crate::testenv::harness();
    let mut previous = active();
    previous.source = WeatherSource::Semantic;
    previous.playlist_id = None;
    previous.query = Some("old sky".into());
    test_status(&state, Some(previous.clone()));
    crate::testenv::write_config(
        serde_json::json!({"weather":{"enabled":true,"source":"semantic","semantic":{"queries":{"clear":"new sky"}}}}),
    );
    note_apply(&state, ApplySource::User, &[String::from("DP-1")], None);
    let mut guard = lock(&RUNTIME);
    let runtime = guard.as_mut().unwrap();
    assert_eq!(runtime.policy.query.as_deref(), Some("new sky"));
    previous.query = Some("new sky".into());
    runtime.update(previous);
    assert!(runtime.assignment("DP-1").is_none());
    assert!(runtime.status.resume_available);
    drop(guard);
    test_status(&state, None);
}

#[test]
fn semantic_query_priority() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(
        serde_json::json!({"weather":{"source":"semantic","semantic":{"queries":{"rainy":"","windy":"windswept trees"}}}}),
    );
    let state = Arc::new(WallState::open().unwrap());
    let config = state.config().clone();
    let weather = vec!["rainy".into(), "windy".into()];
    assert_eq!(selection(&config, &weather), Some(("windy", None, Some("windswept trees".into()))));
}

#[test]
fn approved_survives_missing_lens() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({
        "schedule":{"latitude":59.3},
        "semantic":{"enabled":false},
        "weather":{"enabled":true,"source":"semantic","semantic":{
            "selection":"approved","queries":{"clear":""},
            "approved":{"clear":["static:chosen.png"]}
        }}
    }));
    let state = Arc::new(WallState::open().unwrap());
    cache_item(&state, "chosen.png");
    let forecast =
        Forecast { weather: vec!["clear".into()], fetched_at: Some(42), ..Forecast::default() };
    let result = status(&state, forecast);
    assert_eq!(result.selection, WeatherSelection::Approved);
    assert_eq!(result.status, WeatherState::Active);
    assert_eq!(result.member_keys, ["static:chosen.png"]);
    assert!(result.query.is_none());
}

#[test]
fn approved_empty_holds() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({
        "schedule":{"latitude":59.3},
        "weather":{"enabled":true,"source":"semantic","semantic":{
            "selection":"approved","approved":{"clear":[]}
        }}
    }));
    let state = Arc::new(WallState::open().unwrap());
    let forecast =
        Forecast { weather: vec!["clear".into()], fetched_at: Some(42), ..Forecast::default() };
    let result = status(&state, forecast);
    assert_eq!(result.status, WeatherState::Empty);
    assert!(result.member_keys.is_empty());
}

#[test]
fn approved_tracks_visibility() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({
        "schedule":{"latitude":59.3},
        "weather":{"enabled":true,"source":"semantic","semantic":{
            "selection":"approved","approved":{
                "rainy":["static:rain.png"],"clear":["static:clear.png"]
            }
        }}
    }));
    let state = Arc::new(WallState::open().unwrap());
    cache_item(&state, "rain.png");
    cache_item(&state, "clear.png");
    let rain = Forecast {
        weather: vec!["clear".into(), "rainy".into()],
        fetched_at: Some(42),
        ..Forecast::default()
    };
    let result = status(&state, rain.clone());
    assert_eq!(result.category.as_deref(), Some("rainy"));
    assert_eq!(result.member_keys, ["static:rain.png"]);
    state.with_db(|conn| db::set_hidden(conn, "static:rain.png", true)).unwrap();
    let hidden = status(&state, rain.clone());
    assert_eq!(hidden.category.as_deref(), Some("rainy"));
    assert_eq!(hidden.status, WeatherState::Empty);
    assert_eq!(state.config().weather_approved("rainy"), ["static:rain.png"]);
    state.with_db(|conn| db::set_hidden(conn, "static:rain.png", false)).unwrap();
    assert_eq!(status(&state, rain.clone()).member_keys, ["static:rain.png"]);
    state.with_db(|conn| db::delete_by_name(conn, "rain.png")).unwrap();
    assert_eq!(status(&state, rain.clone()).status, WeatherState::Empty);
    cache_item(&state, "rain.png");
    assert_eq!(status(&state, rain).member_keys, ["static:rain.png"]);
}

#[test]
fn queued_approval_rechecks() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({
        "schedule":{"latitude":59.3},
        "weather":{"enabled":true,"source":"semantic","semantic":{
            "selection":"approved","approved":{"clear":["static:chosen.png","static:missing.png"]}
        }}
    }));
    let state = Arc::new(WallState::open().unwrap());
    cache_item(&state, "chosen.png");
    let forecast =
        Forecast { weather: vec!["clear".into()], fetched_at: Some(42), ..Forecast::default() };
    test_status(&state, Some(status(&state, forecast)));
    let revision = assignment("DP-1").unwrap().1;
    assert!(allows(&state, "DP-1", None, revision, "static:chosen.png"));
    crate::testenv::write_config(serde_json::json!({
        "schedule":{"latitude":59.3},
        "weather":{"enabled":true,"source":"semantic","semantic":{
            "selection":"approved","approved":{"clear":["static:chosen.png"]}
        }}
    }));
    assert!(!allows(&state, "DP-1", None, revision, "static:chosen.png"));
    crate::testenv::write_config(serde_json::json!({
        "schedule":{"latitude":59.3},
        "weather":{"enabled":true,"source":"semantic","semantic":{
            "selection":"approved","approved":{"clear":[]}
        }}
    }));
    assert!(!allows(&state, "DP-1", None, revision, "static:chosen.png"));
    crate::testenv::write_config(serde_json::json!({
        "schedule":{"latitude":59.3},
        "weather":{"enabled":true,"source":"semantic","semantic":{
            "selection":"automatic","approved":{"clear":["static:chosen.png"]}
        }}
    }));
    assert!(!allows(&state, "DP-1", None, revision, "static:chosen.png"));
    test_status(&state, None);
}

#[test]
fn missing_semantic_index() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(
        serde_json::json!({"schedule":{"latitude":59.3},"weather":{"enabled":true,"source":"semantic"}}),
    );
    let state = Arc::new(WallState::open().unwrap());
    let forecast =
        Forecast { weather: vec!["clear".into()], fetched_at: Some(42), ..Forecast::default() };
    let result = status(&state, forecast);
    assert_eq!(result.source, WeatherSource::Semantic);
    assert_eq!(result.query.as_deref(), Some("clear blue sky and sunshine"));
    assert_eq!(result.status, WeatherState::SearchUnavailable);
    assert!(result.member_keys.is_empty());
}

#[test]
fn cold_semantic_snapshot() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(
        serde_json::json!({"schedule":{"latitude":59.3},"weather":{"enabled":true,"source":"semantic","semantic":{"queries":{"clear":"fast cold skyline"}}}}),
    );
    let state = Arc::new(WallState::open().unwrap());
    let forecast =
        Forecast { weather: vec!["clear".into()], fetched_at: Some(42), ..Forecast::default() };
    let fast = status_for(&state, forecast.clone(), false);
    assert_eq!(fast.status, WeatherState::Loading);
    assert_eq!(fast.query.as_deref(), Some("fast cold skyline"));
    assert_eq!(status(&state, forecast).status, WeatherState::SearchUnavailable);
}

#[test]
fn unavailable_holds_pause() {
    let mut runtime = runtime("offline");
    runtime.update(active());
    runtime.policy.paused.insert("DP-1".into());
    runtime.update(WeatherStatus {
        enabled: true,
        status: WeatherState::Unavailable,
        ..WeatherStatus::default()
    });
    assert!(runtime.assignment("DP-2").is_none());
    runtime.update(active());
    assert!(runtime.assignment("DP-1").is_none());
    assert!(runtime.assignment("DP-2").is_some());
}

#[test]
fn fetch_preserves_revision() {
    let mut runtime = runtime("fetch");
    runtime.update(active());
    let revision = runtime.revision;
    let mut next = active();
    next.fetched_at = Some(42);
    runtime.update(next);
    assert_eq!(runtime.revision, revision);
}

#[test]
fn restart_keeps_pause() {
    let mut runtime = runtime("restart");
    runtime.update(active());
    runtime.policy.paused.insert("DP-1".into());
    runtime.save();
    let mut restarted = Runtime::load(runtime.path);
    for status in [WeatherState::Loading, WeatherState::Unavailable] {
        restarted.update(WeatherStatus { enabled: true, status, ..WeatherStatus::default() });
        assert!(restarted.status.resume_available);
    }
    restarted.update(active());
    assert!(restarted.assignment("DP-1").is_none());
    assert!(restarted.assignment("DP-2").is_some());
}

#[test]
fn priority_uses_configured() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(
        serde_json::json!({"general":{"locale":"London"}, "weather":{"enabled":true,"playlists":{"rainy":"7","windy":"8"}}}),
    );
    let state = Arc::new(WallState::open().unwrap());
    let forecast = Forecast {
        weather: vec!["rainy".into(), "windy".into()],
        fetched_at: Some(42),
        ..Forecast::default()
    };
    let result = status(&state, forecast.clone());
    assert_eq!(result.category.as_deref(), Some("rainy"));
    assert_eq!(result.playlist_id, Some(7));
    crate::testenv::write_config(
        serde_json::json!({"general":{"locale":"London"}, "weather":{"enabled":true,"playlists":{"windy":"8"}}}),
    );
    state.reload_config();
    assert_eq!(status(&state, forecast).playlist_id, Some(8));
}

#[test]
fn missing_empty_unmapped() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(
        serde_json::json!({"schedule":{"latitude":59.3},"weather":{"enabled":true}}),
    );
    let state = Arc::new(WallState::open().unwrap());
    let forecast =
        Forecast { weather: vec!["clear".into()], fetched_at: Some(42), ..Forecast::default() };
    assert_eq!(status(&state, forecast.clone()).status, WeatherState::Unmapped);
    let id = state.with_db(|conn| db::playlist_create(conn, "weather empty")).unwrap();
    crate::testenv::write_config(
        serde_json::json!({"schedule":{"latitude":59.3},"weather":{"enabled":true,"playlists":{"clear":id.to_string()}}}),
    );
    state.reload_config();
    assert_eq!(status(&state, forecast.clone()).status, WeatherState::Empty);
    state.with_db(|conn| db::playlist_delete(conn, id)).unwrap();
    assert_eq!(status(&state, forecast).status, WeatherState::Missing);
    assert!(state.with_db(db::playlist_assigns).unwrap().is_empty());
}

#[test]
fn assignments_report_paused() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(
        serde_json::json!({"schedule":{"latitude":59.3},"weather":{"enabled":true}}),
    );
    let state = Arc::new(WallState::open().unwrap());
    let id = state.with_db(|conn| db::playlist_create(conn, "weather status")).unwrap();
    cache_item(&state, "status.png");
    state.with_db(|conn| db::playlist_add_member(conn, id, "static:status.png")).unwrap();
    crate::testenv::write_config(
        serde_json::json!({"schedule":{"latitude":59.3},"weather":{"enabled":true,"playlists":{"clear":id.to_string()}}}),
    );
    state.reload_config();
    let mut runtime = runtime("assignments");
    for output in ["DP-1", "*"] {
        state.with_db(|conn| db::playlist_assign_clear(conn, None)).unwrap();
        state.with_db(|conn| db::playlist_assign_set(conn, output, Some(id))).unwrap();
        let forecast =
            Forecast { weather: vec!["clear".into()], fetched_at: Some(42), ..Forecast::default() };
        runtime.update(status(&state, forecast));
        assert_eq!(runtime.status.status, WeatherState::Paused);
        assert_eq!(runtime.status.paused_outputs, [output]);
        assert!(runtime.policy.paused.is_empty());
        assert!(!runtime.status.resume_available);
    }
    state.with_db(|conn| db::playlist_delete(conn, id)).unwrap();
}

#[test]
fn filter_only_resolves_members() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({"schedule":{"latitude":59.3}}));
    let state = Arc::new(WallState::open().unwrap());
    let id = state.with_db(|conn| db::playlist_create(conn, "weather filter")).unwrap();
    cache_item(&state, "filter.png");
    state.with_db(|conn| db::playlist_add_member(conn, id, "static:filter.png")).unwrap();
    crate::testenv::write_config(
        serde_json::json!({"schedule":{"latitude":59.3},"weather":{"playlists":{"clear":id.to_string()}}}),
    );
    state.reload_config();
    let result = status(
        &state,
        Forecast { weather: vec!["clear".into()], fetched_at: Some(42), ..Forecast::default() },
    );
    assert_eq!(result.status, WeatherState::Disabled);
    assert_eq!(result.member_keys, ["static:filter.png"]);
    assert_eq!(result.resolved_count, 1);
    assert!(!result.enabled);
    state.with_db(|conn| db::playlist_delete(conn, id)).unwrap();
}

fn queued() -> (std::sync::MutexGuard<'static, ()>, Arc<WallState>, i64) {
    let (guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({"weather":{"enabled":true}}));
    let state = Arc::new(WallState::open().unwrap());
    state.with_db(|conn| db::playlist_assign_clear(conn, None)).unwrap();
    let id = state.with_db(|conn| db::playlist_create(conn, "queued weather")).unwrap();
    for key in ["static:queued-a.png", "static:queued-b.png"] {
        cache_item(&state, key.strip_prefix("static:").unwrap());
        state.with_db(|conn| db::playlist_add_member(conn, id, key)).unwrap();
    }
    crate::testenv::write_config(
        serde_json::json!({"weather":{"enabled":true,"playlists":{"clear":id.to_string()}}}),
    );
    state.reload_config();
    let mut current = active();
    current.playlist_id = Some(id);
    current.member_keys = vec!["static:queued-a.png".into(), "static:queued-b.png".into()];
    test_status(&state, Some(current));
    (guard, state, id)
}

#[test]
fn queued_location_is_revalidated() {
    let (_guard, state, id) = queued();
    assert!(allows(&state, "DP-1", Some(id), 1, "static:queued-a.png"));
    crate::testenv::write_config(
        serde_json::json!({"schedule":{"latitude":48.3},"weather":{"enabled":true,"playlists":{"clear":id.to_string()}}}),
    );
    assert!(!allows(&state, "DP-1", Some(id), 1, "static:queued-a.png"));
    state.with_db(|conn| db::playlist_delete(conn, id)).unwrap();
    test_status(&state, None);
}

#[test]
fn queued_members_are_revalidated() {
    let (_guard, state, id) = queued();
    assert!(allows(&state, "DP-1", Some(id), 1, "static:queued-a.png"));
    state.with_db(|conn| db::playlist_remove_member(conn, id, "static:queued-a.png")).unwrap();
    assert!(!allows(&state, "DP-1", Some(id), 1, "static:queued-a.png"));
    assert!(allows(&state, "DP-1", Some(id), 1, "static:queued-b.png"));
    state.with_db(|conn| db::playlist_remove_member(conn, id, "static:queued-b.png")).unwrap();
    assert!(!allows(&state, "DP-1", Some(id), 1, "static:queued-b.png"));
    state.with_db(|conn| db::playlist_delete(conn, id)).unwrap();
    assert!(!allows(&state, "DP-1", Some(id), 1, "static:queued-a.png"));
    test_status(&state, None);
}

#[test]
fn queued_hidden_is_rejected() {
    let (_guard, state, id) = queued();
    state
        .with_db(|conn| {
            db::upsert_cache_entry(
                conn,
                "static:queued-a.png",
                "static",
                "queued-a.png",
                "",
                "",
                "",
                "",
                1,
                0,
                0,
                0,
                1,
                1,
                1,
            )
        })
        .unwrap();
    assert!(allows(&state, "DP-1", Some(id), 1, "static:queued-a.png"));
    state.with_db(|conn| db::set_hidden(conn, "static:queued-a.png", true)).unwrap();
    assert!(!allows(&state, "DP-1", Some(id), 1, "static:queued-a.png"));
    assert!(allows(&state, "DP-1", Some(id), 1, "static:queued-b.png"));
    state.with_db(|conn| db::playlist_delete(conn, id)).unwrap();
    state.with_db(|conn| db::delete_by_name(conn, "queued-a.png")).unwrap();
    test_status(&state, None);
}

#[test]
fn queued_mapping_is_revalidated() {
    let (_guard, state, id) = queued();
    assert!(allows(&state, "DP-1", Some(id), 1, "static:queued-a.png"));
    crate::testenv::write_config(
        serde_json::json!({"weather":{"enabled":true,"playlists":{"clear":(id+1).to_string()}}}),
    );
    state.reload_config();
    assert!(!allows(&state, "DP-1", Some(id), 1, "static:queued-a.png"));
    let mut current = active();
    current.playlist_id = Some(id);
    current.category = Some("windy".into());
    current.weather = vec!["rainy".into(), "windy".into()];
    current.member_keys = vec!["static:queued-a.png".into(), "static:queued-b.png".into()];
    test_status(&state, Some(current));
    crate::testenv::write_config(
        serde_json::json!({"weather":{"enabled":true,"playlists":{"windy":id.to_string()}}}),
    );
    state.reload_config();
    assert!(allows(&state, "DP-1", Some(id), 1, "static:queued-a.png"));
    crate::testenv::write_config(
        serde_json::json!({"weather":{"enabled":true,"playlists":{"rainy":(id+1).to_string(),"windy":id.to_string()}}}),
    );
    state.reload_config();
    assert!(!allows(&state, "DP-1", Some(id), 1, "static:queued-a.png"));
    state.with_db(|conn| db::playlist_delete(conn, id)).unwrap();
    test_status(&state, None);
}

#[test]
fn passive_disabled_is_immediate() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(
        serde_json::json!({"schedule":{"latitude":47.123,"longitude":18.456}}),
    );
    let (state, events, stats) = crate::testenv::harness();
    let before = std::time::Instant::now();
    let response = crate::testenv::call(
        &state,
        &events,
        &stats,
        wall_proto::rpc::WALL_WEATHER,
        serde_json::json!({}),
    );
    let status: WeatherStatus = serde_json::from_value(response.result.unwrap()).unwrap();
    assert!(!status.enabled);
    assert_eq!(status.status, WeatherState::Disabled);
    assert!(before.elapsed() < std::time::Duration::from_secs(1));
    test_status(&state, None);
}

#[test]
fn cold_status_is_loading() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(
        serde_json::json!({"schedule":{"latitude":48.321},"weather":{"enabled":true}}),
    );
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    assert_eq!(snapshot(&ctx).status, WeatherState::Loading);
    assert_eq!(publish(&ctx, Some(Forecast::default()), false).status, WeatherState::Unavailable);
    test_status(&state, None);
}

#[test]
fn busy_snapshot_query() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(
        serde_json::json!({"schedule":{"latitude":59.3},"weather":{"enabled":true,"source":"semantic","semantic":{"queries":{"clear":"new skyline"}}}}),
    );
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let mut previous = active();
    previous.source = WeatherSource::Semantic;
    previous.playlist_id = None;
    previous.query = Some("old skyline".into());
    previous.fetched_at = Some(42);
    test_status(&state, Some(previous));
    let publish_guard = lock(&PUBLISH_LOCK);
    let current = snapshot(&ctx);
    assert_eq!(current.status, WeatherState::Loading);
    assert_eq!(current.weather, ["clear"]);
    assert_eq!(current.fetched_at, Some(42));
    assert_eq!(current.query.as_deref(), Some("new skyline"));
    assert!(current.member_keys.is_empty());
    drop(publish_guard);
    test_status(&state, None);
}

#[test]
fn busy_snapshot_rechecks_approval() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({
        "schedule":{"latitude":59.3},
        "weather":{"enabled":true,"source":"semantic","semantic":{
            "selection":"approved","approved":{"clear":["static:chosen.png","static:missing.png"]}
        }}
    }));
    let (state, events, stats) = crate::testenv::harness();
    cache_item(&state, "chosen.png");
    let ctx = crate::testenv::context(&state, &events, &stats);
    let forecast =
        Forecast { weather: vec!["clear".into()], fetched_at: Some(42), ..Forecast::default() };
    test_status(&state, Some(status(&state, forecast)));
    let publish_guard = lock(&PUBLISH_LOCK);
    crate::testenv::write_config(serde_json::json!({
        "schedule":{"latitude":59.3},
        "weather":{"enabled":true,"source":"semantic","semantic":{
            "selection":"approved","approved":{"clear":["static:chosen.png"]}
        }}
    }));
    let current = snapshot(&ctx);
    assert_eq!(current.status, WeatherState::Loading);
    assert!(current.member_keys.is_empty());
    drop(publish_guard);
    test_status(&state, None);
}

#[test]
fn candidates_leave_forecast() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({
        "schedule":{"latitude":59.3},
        "weather":{"enabled":true,"source":"semantic","semantic":{
            "selection":"approved","approved":{"clear":["static:chosen.png"]}
        }}
    }));
    let (state, events, stats) = crate::testenv::harness();
    cache_item(&state, "chosen.png");
    let forecast =
        Forecast { weather: vec!["clear".into()], fetched_at: Some(42), ..Forecast::default() };
    let status = status(&state, forecast);
    test_status(&state, Some(status.clone()));
    let bad = crate::testenv::call(
        &state,
        &events,
        &stats,
        wall_proto::rpc::WALL_WEATHER_CANDIDATES,
        serde_json::json!({"category":"sunny"}),
    );
    assert!(bad.error.is_some());
    let response = crate::testenv::call(
        &state,
        &events,
        &stats,
        wall_proto::rpc::WALL_WEATHER_CANDIDATES,
        serde_json::json!({"category":"rainy"}),
    );
    let result: WeatherCandidates = serde_json::from_value(response.result.unwrap()).unwrap();
    assert_eq!(result.category, "rainy");
    assert!(result.error.is_some());
    assert_eq!(lock(&RUNTIME).as_ref().unwrap().status, status);
    test_status(&state, None);
}

#[test]
fn busy_snapshot_keeps_members() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(
        serde_json::json!({"schedule":{"latitude":59.3},"weather":{"enabled":true,"source":"semantic","semantic":{"queries":{"clear":"old skyline"}}}}),
    );
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let mut previous = active();
    previous.source = WeatherSource::Semantic;
    previous.playlist_id = None;
    previous.query = Some("old skyline".into());
    previous.fetched_at = Some(42);
    test_status(&state, Some(previous.clone()));
    let publish_guard = lock(&PUBLISH_LOCK);
    let current = snapshot(&ctx);
    assert_eq!(current.status, WeatherState::Active);
    assert_eq!(current.query, previous.query);
    assert_eq!(current.member_keys, previous.member_keys);
    drop(publish_guard);
    crate::testenv::write_config(
        serde_json::json!({"schedule":{"latitude":48.3},"weather":{"enabled":true,"source":"semantic","semantic":{"queries":{"clear":"old skyline"}}}}),
    );
    let publish_guard = lock(&PUBLISH_LOCK);
    let changed = snapshot(&ctx);
    assert_eq!(changed.status, WeatherState::Loading);
    assert!(changed.weather.is_empty());
    assert_eq!(changed.fetched_at, None);
    assert_eq!(changed.category, None);
    assert_eq!(changed.query, None);
    assert!(changed.member_keys.is_empty());
    drop(publish_guard);
    test_status(&state, None);
}

#[test]
fn location_changes_during_search() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(
        serde_json::json!({"schedule":{"latitude":59.3},"weather":{"enabled":true,"source":"semantic","semantic":{"queries":{"clear":"old skyline"}}}}),
    );
    let state = Arc::new(WallState::open().unwrap());
    let (started_tx, started_rx) = std::sync::mpsc::sync_channel(0);
    let (finished_tx, finished_rx) = std::sync::mpsc::sync_channel(0);
    let writer = std::thread::spawn(move || {
        started_rx.recv().unwrap();
        crate::testenv::write_config(
            serde_json::json!({"schedule":{"latitude":48.3},"weather":{"enabled":true,"source":"semantic","semantic":{"queries":{"clear":"old skyline"}}}}),
        );
        finished_tx.send(()).unwrap();
    });
    let forecast =
        Forecast { weather: vec!["clear".into()], fetched_at: Some(42), ..Forecast::default() };
    let result = status_with(&state, forecast, true, |_, _, _, _| {
        started_tx.send(()).unwrap();
        finished_rx.recv().unwrap();
        Some(Ok(vec![String::from("static:old.png")]))
    });
    writer.join().unwrap();
    assert_eq!(result.status.status, WeatherState::Loading);
    assert!(result.status.weather.is_empty());
    assert!(result.status.member_keys.is_empty());
    let mut runtime = runtime("location-switch");
    runtime.update(result.status);
    assert!(runtime.assignment("DP-1").is_none());
}

#[test]
fn publication_rechecks_location() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(
        serde_json::json!({"schedule":{"latitude":59.3},"weather":{"enabled":true,"source":"semantic","semantic":{"queries":{"clear":"old skyline"}}}}),
    );
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let forecast =
        Forecast { weather: vec!["clear".into()], fetched_at: Some(42), ..Forecast::default() };
    let resolved = status_with(&state, forecast, true, |_, _, _, _| {
        Some(Ok(vec![String::from("static:old.png")]))
    });
    assert_eq!(resolved.status.status, WeatherState::Active);
    crate::testenv::write_config(
        serde_json::json!({"schedule":{"latitude":48.3},"weather":{"enabled":true,"source":"semantic","semantic":{"queries":{"clear":"old skyline"}}}}),
    );
    let published = publish_resolved(&ctx, resolved, false, false);
    assert_eq!(published.status, WeatherState::Loading);
    assert!(published.member_keys.is_empty());
    assert_eq!(lock(&RUNTIME).as_ref().unwrap().location, Some(location(&state.config())));
    test_status(&state, None);
}

#[test]
fn location_renews_revision() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(
        serde_json::json!({"schedule":{"latitude":59.3},"weather":{"enabled":true,"source":"semantic","semantic":{"queries":{"clear":"old skyline"}}}}),
    );
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let forecast =
        Forecast { weather: vec!["clear".into()], fetched_at: Some(42), ..Forecast::default() };
    let first = status_with(&state, forecast.clone(), true, |_, _, _, _| {
        Some(Ok(vec![String::from("static:old.png")]))
    });
    let published = publish_resolved(&ctx, first, false, false);
    let previous = lock(&RUNTIME).as_ref().unwrap().revision;
    crate::testenv::write_config(
        serde_json::json!({"schedule":{"latitude":48.3},"weather":{"enabled":true,"source":"semantic","semantic":{"queries":{"clear":"old skyline"}}}}),
    );
    state.reload_config();
    let next = status_with(&state, forecast, true, |_, _, _, _| {
        Some(Ok(vec![String::from("static:old.png")]))
    });
    let updated = publish_resolved(&ctx, next, false, false);
    let current = lock(&RUNTIME).as_ref().unwrap().revision;
    assert_eq!(published, updated);
    assert_ne!(previous, current);
    assert_eq!(assignment("DP-1"), Some((None, current)));
    test_status(&state, None);
}

#[test]
fn smart_tags_refresh_members() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(
        serde_json::json!({"schedule":{"latitude":59.3},"weather":{"enabled":true}}),
    );
    let (state, events, stats) = crate::testenv::harness();
    let id = state.with_db(|conn| db::playlist_create(conn, "weather tags")).unwrap();
    state
        .with_db(|conn| {
            db::playlist_update(conn, id, None, Some("smart"), Some("tag:weather-test"), None, None)
        })
        .unwrap();
    let key = "static:weather-tags.png";
    cache_item(&state, "weather-tags.png");
    crate::testenv::write_config(
        serde_json::json!({"schedule":{"latitude":59.3},"weather":{"enabled":true,"playlists":{"clear":id.to_string()}}}),
    );
    state.reload_config();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let forecast =
        Forecast { weather: vec!["clear".into()], fetched_at: Some(42), ..Forecast::default() };
    assert_eq!(publish(&ctx, Some(forecast.clone()), false).status, WeatherState::Empty);
    let executor = tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap();
    for (tags, count) in [("weather-test", 1), ("", 0), ("weather-test", 1)] {
        executor.block_on(async {
            while tokio::time::timeout(std::time::Duration::ZERO, WAKE.notified()).await.is_ok() {}
        });
        let response = crate::testenv::call(
            &state,
            &events,
            &stats,
            wall_proto::rpc::WALL_UPDATE_TAGS,
            serde_json::json!({"key":key,"tags":tags}),
        );
        assert!(response.error.is_none());
        executor.block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(1), WAKE.notified()).await.unwrap();
        });
        assert_eq!(publish(&ctx, Some(forecast.clone()), false).resolved_count, count);
    }
    state.with_db(|conn| db::playlist_delete(conn, id)).unwrap();
    state.with_db(|conn| db::delete_by_name(conn, "weather-tags.png")).unwrap();
    test_status(&state, None);
}

#[test]
fn curated_waits_for_scan() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(
        serde_json::json!({"schedule":{"latitude":59.3},"weather":{"enabled":true}}),
    );
    let (state, events, stats) = crate::testenv::harness();
    let id = state.with_db(|conn| db::playlist_create(conn, "weather scan")).unwrap();
    state.with_db(|conn| db::playlist_add_member(conn, id, "static:weather-scan/a.png")).unwrap();
    crate::testenv::write_config(
        serde_json::json!({"schedule":{"latitude":59.3},"weather":{"enabled":true,"playlists":{"clear":id.to_string()}}}),
    );
    state.reload_config();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let forecast =
        Forecast { weather: vec!["clear".into()], fetched_at: Some(42), ..Forecast::default() };
    assert_eq!(publish(&ctx, Some(forecast.clone()), false).status, WeatherState::Empty);
    let executor = tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap();
    executor.block_on(async {
        while tokio::time::timeout(std::time::Duration::ZERO, WAKE.notified()).await.is_ok() {}
    });
    cache_item(&state, "weather-scan/a.png");
    let response = crate::testenv::call(
        &state,
        &events,
        &stats,
        wall_proto::rpc::SCAN_DONE,
        serde_json::json!({"count":1}),
    );
    assert!(response.error.is_none());
    executor.block_on(async {
        tokio::time::timeout(std::time::Duration::from_secs(1), WAKE.notified()).await.unwrap();
    });
    let updated = publish(&ctx, Some(forecast), false);
    assert_eq!(updated.status, WeatherState::Active);
    assert_eq!(updated.member_keys, ["static:weather-scan/a.png"]);
    state.with_db(|conn| db::playlist_delete(conn, id)).unwrap();
    state.with_db(|conn| db::delete_by_name(conn, "weather-scan/a.png")).unwrap();
    test_status(&state, None);
}

#[test]
fn retired_filter_skips_search() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({
        "schedule":{"latitude":59.3},
        "weather":{"source":"semantic","enabled":false},
        "general":{"weatherMatch":true}
    }));
    let state = Arc::new(WallState::open().unwrap());
    let result = status_with(
        &state,
        Forecast { weather: vec!["clear".into()], fetched_at: Some(42), ..Forecast::default() },
        true,
        |_, _, _, _| panic!("retired filter invoked search"),
    );
    assert_eq!(result.status.status, WeatherState::Disabled);
    assert!(result.status.member_keys.is_empty());
}
