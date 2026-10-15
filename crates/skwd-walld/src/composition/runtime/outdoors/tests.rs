use super::*;
use crate::infrastructure::weather::Observation;
use serde_json::json;

fn setup() -> (Arc<WallState>, crate::composition::context::Ctx) {
    crate::testenv::write_config(
        json!({"pickOnlyMode":true,"schedule":{"latitude":59.3},"semantic":{"enabled":false},"weather":{"enabled":true,"source":"outdoors"}}),
    );
    let (state, events, stats) = crate::testenv::harness();
    state.with_db(|conn| db::playlist_assign_clear(conn, None)).unwrap();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let _ = std::fs::remove_file(PathBuf::from(state.config().cache_dir()).join(SESSION_FILE));
    *lock(&RUNTIME) = Some(load(&state));
    (state, ctx)
}

fn summer() -> Context {
    Context {
        season: wall_proto::OutdoorSeason::Summer,
        light: wall_proto::LightPhase::Day,
        category: "cloudy".into(),
        snow: false,
    }
}

fn item(key: &str) -> WallpaperItem {
    WallpaperItem { key: Some(key.into()), name: Some(key.into()), ..WallpaperItem::default() }
}

#[test]
fn context_uses_forecast_time() {
    let forecast = Forecast {
        fetched_at: Some(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()),
        weather: vec!["cloudy".into()],
        observation: Some(Observation {
            latitude: -34.0,
            longitude: 18.0,
            observed_at: Some(1_781_524_800),
            ..Observation::default()
        }),
        ..Forecast::default()
    };
    assert_eq!(context(&forecast).unwrap().season, wall_proto::OutdoorSeason::Winter);
}

#[test]
fn empty_library_uses_starter() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, _) = setup();
    let (items, kind) = candidate_items(&state, &summer(), true);
    assert!(!items.is_empty());
    assert!(matches!(kind, OutdoorMatch::Starter | OutdoorMatch::Ambient));
    assert!(items.iter().all(|item| std::path::Path::new(item.path.as_deref().unwrap()).is_file()));
}

#[test]
fn same_context_holds_generation() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, _) = setup();
    let mut runtime = load(&state);
    let items = vec![item("static:one"), item("static:two")];
    assert!(reconcile(
        &mut runtime,
        &state,
        &summer(),
        &Forecast::default(),
        items.clone(),
        OutdoorMatch::Library
    ));
    let generation = runtime.generation;
    assert!(!reconcile(
        &mut runtime,
        &state,
        &summer(),
        &Forecast::default(),
        items,
        OutdoorMatch::Library
    ));
    assert_eq!(runtime.generation, generation);
}

#[test]
fn refresh_keeps_selected_wallpaper() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, _) = setup();
    skwd_wall_core::audio::write_state(
        &state.config().cache_dir(),
        &json!({"*":{"type":"static","path":"/wall/one.jpg"}}),
    );
    let first = "static:/wall/one.jpg";
    let mut runtime = load(&state);
    runtime.context = Some(summer());
    reconcile(
        &mut runtime,
        &state,
        &summer(),
        &Forecast::default(),
        vec![item(first), item("static:/wall/two.jpg")],
        OutdoorMatch::Library,
    );
    assert!(!runtime.due.contains("*"));
    let generation = runtime.generation;
    reconcile(
        &mut runtime,
        &state,
        &summer(),
        &Forecast::default(),
        vec![item("static:/wall/two.jpg"), item(first)],
        OutdoorMatch::Library,
    );
    assert!(!runtime.due.contains("*"));
    assert_eq!(runtime.generation, generation);
    reconcile(
        &mut runtime,
        &state,
        &summer(),
        &Forecast::default(),
        vec![item("static:/wall/two.jpg")],
        OutdoorMatch::Library,
    );
    assert!(!runtime.due.contains("*"));
}

#[test]
fn context_change_queues_once() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, _) = setup();
    let first = "static:/wall/one.jpg";
    let second = "static:/wall/two.jpg";
    skwd_wall_core::audio::write_state(
        &state.config().cache_dir(),
        &json!({"*":{"type":"static","path":"/wall/one.jpg"}}),
    );
    let mut runtime = load(&state);
    runtime.session.contexts.insert("*".into(), summer());
    runtime.context = Some(summer());
    let mut next = summer();
    next.category = "rainy".into();
    reconcile(
        &mut runtime,
        &state,
        &next,
        &Forecast::default(),
        vec![item(second)],
        OutdoorMatch::Library,
    );
    assert!(runtime.due.contains("*"));
    runtime.session.contexts.insert("*".into(), next.clone());
    reconcile(
        &mut runtime,
        &state,
        &next,
        &Forecast::default(),
        vec![item(second)],
        OutdoorMatch::Library,
    );
    assert!(!runtime.due.contains("*"));
    assert_eq!(current_keys(&state)["*"], first);
}

#[test]
fn restart_keeps_context() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, _) = setup();
    let first = "static:/wall/one.jpg";
    skwd_wall_core::audio::write_state(
        &state.config().cache_dir(),
        &json!({"*":{"type":"static","path":"/wall/one.jpg"}}),
    );
    let mut runtime = load(&state);
    runtime.session.contexts.insert("*".into(), summer());
    runtime.session.location_key = Some(location_key(&state.config()));
    runtime.session.save(&runtime.path).unwrap();
    let mut restored = load(&state);
    assert_eq!(restored.context, Some(summer()));
    reconcile(
        &mut restored,
        &state,
        &summer(),
        &Forecast::default(),
        vec![item("static:/wall/two.jpg"), item(first)],
        OutdoorMatch::Library,
    );
    assert!(!restored.due.contains("*"));
    assert_eq!(restored.plans["*"][0], first);
}

#[test]
fn exclusion_invalidates_queued_apply() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, ctx) = setup();
    let revision = {
        let mut guard = lock(&RUNTIME);
        let runtime = guard.as_mut().unwrap();
        reconcile(
            runtime,
            &state,
            &summer(),
            &Forecast::default(),
            vec![item("static:one")],
            OutdoorMatch::Library,
        );
        runtime.generation
    };
    feedback(&ctx, wall_proto::rpc::WALL_WEATHER_EXCLUDE, "*", Some("static:one")).unwrap();
    assert!(!allows(&state, "*", revision, "static:one"));
    assert!(keys("*").is_empty());
}

#[test]
fn source_change_invalidates_queue() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, _) = setup();
    crate::testenv::write_config(json!({"weather":{"enabled":true,"source":"semantic"}}));
    state.reload_config();
    assert!(!allows(&state, "*", 1, "static:one"));
}

#[test]
fn follows_restores_settings() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, ctx) = setup();
    crate::testenv::write_config(
        json!({"pickOnlyMode":true,"weather":{"source":"semantic","enabled":false},"general":{"weatherMatch":false}}),
    );
    state.reload_config();
    let id = state.with_db(|conn| db::playlist_create(conn, "previous")).unwrap();
    state.with_db(|conn| db::playlist_assign_set(conn, "*", Some(id))).unwrap();
    follow(&ctx, true).unwrap();
    assert!(active(&state.config()));
    assert!(state.with_db(db::playlist_assigns).unwrap().is_empty());
    follow(&ctx, false).unwrap();
    assert_eq!(state.config().weather_source(), WeatherSource::Semantic);
    assert!(!state.config().weather_enabled());
    assert!(!state.config().weather_match());
    assert_eq!(state.with_db(db::playlist_assigns).unwrap(), [("*".into(), id)]);
}

#[test]
fn session_survives_restart() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, ctx) = setup();
    follow(&ctx, true).unwrap();
    feedback(&ctx, wall_proto::rpc::WALL_WEATHER_PIN, "*", None).unwrap();
    feedback(&ctx, wall_proto::rpc::WALL_WEATHER_EXCLUDE, "*", Some("static:one")).unwrap();
    let restored = load(&state);
    assert!(restored.session.active);
    assert!(restored.session.held.contains("*"));
    assert!(restored.session.excluded.contains("static:one"));
}

#[test]
fn manual_choice_stops_follow() {
    let (_guard, root) = crate::testenv::lock();
    for source in
        [ApplySource::User, ApplySource::UserOverride, ApplySource::Replay, ApplySource::Random]
    {
        let (state, ctx) = setup();
        let id = state.with_db(|conn| db::playlist_create(conn, "previous")).unwrap();
        state.with_db(|conn| db::playlist_assign_set(conn, "*", Some(id))).unwrap();
        let prior = root.join("prior.png");
        std::fs::write(&prior, b"prior").unwrap();
        skwd_wall_core::audio::write_state(
            &state.config().cache_dir(),
            &json!({"*":{"type":"static","path":prior}}),
        );
        follow(&ctx, true).unwrap();
        let chosen = root.join("chosen.png");
        std::fs::write(&chosen, b"chosen").unwrap();
        assert!(apply(&ctx, &format!("static:{}", chosen.display()), source));
        assert!(!active(&state.config()));
        assert_eq!(state.config().weather_source(), WeatherSource::Outdoors);
        assert!(state.with_db(db::playlist_assigns).unwrap().is_empty());
        assert!(!load(&state).session.active);
        assert_eq!(
            crate::infrastructure::persistence::last_any_source().as_deref(),
            chosen.to_str()
        );
    }
}

#[test]
fn locked_choice_keeps_follow() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, ctx) = setup();
    crate::testenv::write_config(json!({
        "pickOnlyMode":true,
        "weather":{"enabled":true,"source":"outdoors"},
        "display":{"outputLocks":{"DP-1":true}}
    }));
    state.reload_config();
    follow(&ctx, true).unwrap();
    assert!(crate::composition::apply::apply_by_key(
        &state,
        ctx.wallpaper.as_ref(),
        ctx.history.as_ref(),
        ctx.events.as_ref(),
        &ctx.stats,
        "static:/missing.jpg",
        "DP-1",
        false,
        ApplySource::User,
    ));
    assert!(active(&state.config()));
    assert!(load(&state).session.active);
}

#[test]
fn persistence_failure_keeps_follow() {
    let (_guard, root) = crate::testenv::lock();
    for config_failure in [false, true] {
        let (state, ctx) = setup();
        follow(&ctx, true).unwrap();
        let chosen = root.join("failure.png");
        std::fs::write(&chosen, b"chosen").unwrap();
        let path = if config_failure {
            skwd_wall_core::config::config_path()
        } else {
            PathBuf::from(state.config().cache_dir()).join(SESSION_FILE)
        };
        let original = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(!apply(&ctx, &format!("static:{}", chosen.display()), ApplySource::User));
        std::fs::remove_dir(&path).unwrap();
        std::fs::write(&path, original).unwrap();
        state.reload_config();
        assert!(active(&state.config()));
        assert!(load(&state).session.active);
        assert_ne!(
            crate::infrastructure::persistence::last_any_source().as_deref(),
            chosen.to_str()
        );
    }
}

#[test]
fn failed_choice_restores_follow() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, ctx) = setup();
    crate::testenv::write_config(json!({"weather":{"enabled":true,"source":"outdoors"}}));
    state.reload_config();
    follow(&ctx, true).unwrap();
    let previous = current_keys(&state);
    assert!(!apply(&ctx, "static:/missing-manual.jpg", ApplySource::User));
    assert!(active(&state.config()));
    assert!(load(&state).session.active);
    assert_eq!(current_keys(&state), previous);
}

#[test]
fn manual_invalidates_queued_weather() {
    let (_guard, root) = crate::testenv::lock();
    let (state, ctx) = setup();
    follow(&ctx, true).unwrap();
    let next = crate::infrastructure::outdoors::items(&state.config()).remove(0);
    let key = next.key.clone().unwrap();
    let revision = {
        let mut guard = lock(&RUNTIME);
        let runtime = guard.as_mut().unwrap();
        reconcile(
            runtime,
            &state,
            &summer(),
            &Forecast::default(),
            vec![next],
            OutdoorMatch::Starter,
        );
        runtime.generation
    };
    assert!(allows(&state, "*", revision, &key));
    let chosen = root.join("queued.png");
    std::fs::write(&chosen, b"chosen").unwrap();
    assert!(apply(&ctx, &format!("static:{}", chosen.display()), ApplySource::User));
    assert!(!apply(&ctx, &key, ApplySource::WeatherSemantic { revision }));
    assert_eq!(crate::infrastructure::persistence::last_any_source().as_deref(), chosen.to_str());
}

#[test]
fn next_follow_captures_fresh() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, ctx) = setup();
    follow(&ctx, true).unwrap();
    ManualChoice::begin(&state, ApplySource::User).unwrap().commit(ctx.events.as_ref());
    let id = state.with_db(|conn| db::playlist_create(conn, "fresh")).unwrap();
    state.with_db(|conn| db::playlist_assign_set(conn, "*", Some(id))).unwrap();
    skwd_wall_core::audio::write_state(
        &state.config().cache_dir(),
        &json!({"*":{"type":"static","path":"/fresh.jpg"}}),
    );
    follow(&ctx, true).unwrap();
    let guard = lock(&RUNTIME);
    let session = &guard.as_ref().unwrap().session;
    assert!(session.active);
    assert_eq!(session.wallpapers["*"], "static:/fresh.jpg");
    assert_eq!(session.assignments, [("*".into(), id)]);
}

#[test]
fn offline_choice_stops_follow() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, ctx) = setup();
    follow(&ctx, true).unwrap();
    let path = crate::infrastructure::restore_policy::monitors_path();
    std::fs::write(&path, br#"{"offline":{"connector":"DP-2"}}"#).unwrap();
    let response = crate::composition::apply::apply_core(
        &state,
        ctx.wallpaper.as_ref(),
        ctx.history.as_ref(),
        ctx.events.as_ref(),
        &ctx.stats,
        wall_proto::kind::STATIC,
        "/offline.jpg",
        "",
        true,
        100,
        ApplySource::User,
        "@monitor:offline",
        false,
        false,
        None,
        None,
    )
    .unwrap();
    assert_eq!(response["offline"], true);
    assert!(!active(&state.config()));
    assert!(!load(&state).session.active);
    let monitors: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(monitors["offline"]["wallpaper"]["path"], "/offline.jpg");
}

#[test]
fn manual_choice_cancels_restore() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, ctx) = setup();
    crate::testenv::write_config(json!({
        "pickOnlyMode":true,
        "weather":{"enabled":false,"source":"playlists"}
    }));
    state.reload_config();
    follow(&ctx, true).unwrap();
    follow(&ctx, false).unwrap();
    assert!(!active(&state.config()));
    let revision = {
        let mut guard = lock(&RUNTIME);
        let runtime = guard.as_mut().unwrap();
        runtime.restore_config = Some(state.config().clone());
        runtime.generation
    };
    super::super::weather::note_apply(&state, ApplySource::User, &["*".into()], None);
    let guard = lock(&RUNTIME);
    let runtime = guard.as_ref().unwrap();
    assert!(runtime.restore_config.is_none());
    assert_ne!(runtime.generation, revision);
}

#[test]
fn stop_preserves_manual_settings() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, ctx) = setup();
    crate::testenv::write_config(
        json!({"pickOnlyMode":true,"weather":{"source":"playlists","enabled":false},"general":{"weatherMatch":true}}),
    );
    state.reload_config();
    follow(&ctx, true).unwrap();
    crate::testenv::write_config(
        json!({"pickOnlyMode":true,"weather":{"source":"semantic","enabled":false},"general":{"weatherMatch":false}}),
    );
    follow(&ctx, false).unwrap();
    assert_eq!(state.config().weather_source(), WeatherSource::Semantic);
    assert!(!state.config().weather_enabled());
    assert!(!state.config().weather_match());
}

fn apply(ctx: &Ctx, key: &str, source: ApplySource) -> bool {
    crate::composition::apply::apply_by_key(
        &ctx.state,
        ctx.wallpaper.as_ref(),
        ctx.history.as_ref(),
        ctx.events.as_ref(),
        &ctx.stats,
        key,
        "*",
        false,
        source,
    )
}

#[test]
fn stop_restores_after_apply() {
    let (_guard, root) = crate::testenv::lock();
    let (state, ctx) = setup();
    crate::testenv::write_config(json!({"pickOnlyMode":true,"theme":{"policy":"off"}}));
    state.reload_config();
    let prior = root.join("prior.png");
    std::fs::write(&prior, b"original").unwrap();
    let prior = format!("static:{}", prior.display());
    assert!(apply(&ctx, &prior, ApplySource::User));
    skwd_wall_core::audio::write_state(
        &state.config().cache_dir(),
        &json!({"*":{"type":"static","path":prior.strip_prefix("static:").unwrap()}}),
    );
    follow(&ctx, true).unwrap();
    let next = crate::infrastructure::outdoors::items(&state.config())
        .first()
        .unwrap()
        .key
        .clone()
        .unwrap();
    let revision = {
        let mut guard = lock(&RUNTIME);
        let runtime = guard.as_mut().unwrap();
        runtime.plans.insert("*".into(), vec![next.clone()]);
        runtime.generation
    };
    assert!(apply(&ctx, &next, ApplySource::WeatherSemantic { revision }));
    assert!(active(&state.config()));
    assert_eq!(lock(&RUNTIME).as_ref().unwrap().session.owned.get("*"), Some(&next));
    skwd_wall_core::audio::write_state(
        &state.config().cache_dir(),
        &json!({"*":{"type":"static","path":next.strip_prefix("static:").unwrap()}}),
    );
    state.apply().cancel_restore(None);
    assert!(!apply(&ctx, &prior, ApplySource::Restore));
    assert_eq!(assignment("*").unwrap().1, revision);
    follow(&ctx, false).unwrap();
    assert_eq!(
        crate::infrastructure::persistence::last_any_source().as_deref(),
        prior.strip_prefix("static:")
    );
}

#[test]
fn retheme_choice_scopes_apply() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, ctx) = setup();
    let items = crate::infrastructure::outdoors::items(&state.config());
    let first = items[0].key.as_ref().unwrap();
    let second = items[1].key.as_ref().unwrap();
    let revision = {
        let mut guard = lock(&RUNTIME);
        let runtime = guard.as_mut().unwrap();
        runtime.plans.insert("*".into(), vec![first.clone(), second.clone()]);
        runtime.generation
    };
    state.theme().set_source("sentinel-theme");
    let theme_generation = state.theme().source_generation();
    assert!(apply(&ctx, first, ApplySource::WeatherSemantic { revision }));
    assert_ne!(state.theme().source_generation(), theme_generation);
    assert_eq!(state.theme().source().as_deref(), Some("sentinel-theme"));
    crate::testenv::write_config(json!({
        "pickOnlyMode":true,
        "weather":{"enabled":true,"source":"outdoors","outdoors":{"retheme":true}},
        "theme":{"policy":"off"}
    }));
    state.reload_config();
    {
        let mut guard = lock(&RUNTIME);
        guard.as_mut().unwrap().config = state.config().clone();
    }
    assert!(apply(&ctx, second, ApplySource::WeatherSemantic { revision }));
    assert_ne!(state.theme().source().as_deref(), Some("sentinel-theme"));
    assert_eq!(state.config().theme().policy(), "off");
}

#[test]
fn missing_session_resets_source() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, ctx) = setup();
    skwd_wall_core::audio::write_state(
        &state.config().cache_dir(),
        &json!({"*":{"type":"static","path":"/manual.jpg"}}),
    );
    for enabled in [true, false] {
        crate::testenv::write_config(json!({
            "weather":{"source":"outdoors","enabled":enabled},
            "general":{"weatherMatch":true}
        }));
        state.reload_config();
        if !enabled {
            std::fs::write(
                PathBuf::from(state.config().cache_dir()).join(SESSION_FILE),
                b"invalid session",
            )
            .unwrap();
        }
        *lock(&RUNTIME) = Some(load(&state));
        let status = follow(&ctx, false).unwrap();
        assert_eq!(state.config().weather_source(), WeatherSource::Playlists);
        assert!(!state.config().weather_enabled());
        assert!(state.config().weather_match());
        assert_eq!(status.source, WeatherSource::Playlists);
        assert!(status.outdoors.is_none());
        assert_eq!(current_keys(&state)["*"], "static:/manual.jpg");
    }
}

#[test]
fn missing_session_preserves_settings() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, ctx) = setup();
    crate::testenv::write_config(json!({
        "weather":{"source":"semantic","enabled":true},
        "general":{"weatherMatch":true}
    }));
    follow(&ctx, false).unwrap();
    assert_eq!(state.config().weather_source(), WeatherSource::Semantic);
    assert!(state.config().weather_enabled());
    assert!(state.config().weather_match());
}

#[test]
fn restore_rechecks_current_owner() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, _) = setup();
    let revision = {
        let mut guard = lock(&RUNTIME);
        let runtime = guard.as_mut().unwrap();
        runtime.restore_config = Some(state.config().clone());
        runtime.session.wallpapers.insert("*".into(), "static:prior".into());
        runtime.session.owned.insert("*".into(), "static:owned".into());
        runtime.generation
    };
    skwd_wall_core::audio::write_state(
        &state.config().cache_dir(),
        &json!({"*":{"type":"static","path":"manual"}}),
    );
    assert!(!restore_allows(&state, "*", revision, "static:prior"));
}

#[test]
fn refresh_preserves_pending_restore() {
    let (_guard, root) = crate::testenv::lock();
    let (state, ctx) = setup();
    let enabled = state.config().clone();
    crate::testenv::write_config(json!({
        "pickOnlyMode": true,
        "weather": {"enabled": false, "source": "outdoors"}
    }));
    state.reload_config();
    let prior = root.join("refresh-restore.png");
    std::fs::write(&prior, b"prior").unwrap();
    let key = format!("static:{}", prior.display());
    skwd_wall_core::audio::write_state(
        &state.config().cache_dir(),
        &json!({"*":{"type":"static","path":"/owned.jpg"}}),
    );
    let revision = {
        let mut runtime = load(&state);
        runtime.config = enabled;
        runtime.restore_config = Some(state.config().clone());
        runtime.context = Some(summer());
        runtime.session.wallpapers.insert("*".into(), key.clone());
        runtime.session.owned.insert("*".into(), "static:/owned.jpg".into());
        let revision = runtime.generation;
        *lock(&RUNTIME) = Some(runtime);
        revision
    };
    assert!(restore_allows(&state, "*", revision, &key));
    refresh(&ctx, false);
    assert!(restore_allows(&state, "*", revision, &key));
    assert!(apply(&ctx, &key, ApplySource::OutdoorsRestore { revision }));
    assert_eq!(crate::infrastructure::persistence::last_any_source().as_deref(), prior.to_str());
}

#[test]
fn stop_skips_deleted_playlist() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, ctx) = setup();
    let id = state.with_db(|conn| db::playlist_create(conn, "previous")).unwrap();
    state.with_db(|conn| db::playlist_assign_set(conn, "*", Some(id))).unwrap();
    follow(&ctx, true).unwrap();
    state.with_db(|conn| db::playlist_delete(conn, id)).unwrap();
    follow(&ctx, false).unwrap();
    assert!(state.with_db(db::playlist_assigns).unwrap().is_empty());
}

#[test]
fn last_starter_uses_fallback() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, ctx) = setup();
    let (first, kind) = candidate_items(&state, &summer(), true);
    assert_eq!(kind, OutdoorMatch::Starter);
    for item in first {
        feedback(&ctx, wall_proto::rpc::WALL_WEATHER_EXCLUDE, "*", item.key.as_deref()).unwrap();
    }
    let (fallback, kind) = candidate_items(&state, &summer(), true);
    assert_eq!(kind, OutdoorMatch::Ambient);
    assert!(!fallback.is_empty());
}

#[test]
fn snow_requires_independent_weather() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, _) = setup();
    let context = Context { season: wall_proto::OutdoorSeason::Winter, snow: true, ..summer() };
    let (items, kind) = candidate_items(&state, &context, true);
    assert_eq!(kind, OutdoorMatch::Starter);
    assert!(items.iter().all(|item| !item.path.as_deref().unwrap().contains("winter_lake")));
    let clear = Context { category: wall_proto::weather_category::CLEAR.into(), ..context };
    let (items, _) = candidate_items(&state, &clear, true);
    assert!(items.iter().any(|item| item.path.as_deref().unwrap().contains("winter_lake")));
}

#[test]
fn feedback_waits_for_apply() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, ctx) = setup();
    let apply = state.apply().lock();
    let (started, waiting) = std::sync::mpsc::channel();
    let (finished, result) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        started.send(()).unwrap();
        let status = feedback(&ctx, wall_proto::rpc::WALL_WEATHER_PIN, "*", None);
        finished.send(status.is_ok()).unwrap();
    });
    waiting.recv().unwrap();
    assert!(result.recv_timeout(Duration::from_millis(50)).is_err());
    drop(apply);
    assert!(result.recv_timeout(Duration::from_secs(5)).unwrap());
    worker.join().unwrap();
}

#[test]
fn settings_survive_pack_loss() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, _) = setup();
    let key = skwd_config::keys::semantic::ENABLED.to_owned();
    let session = Session {
        settings: vec![(key.clone(), Value::Bool(false))],
        applied_settings: vec![(key.clone(), Value::Bool(true))],
        ..Session::default()
    };
    save_config(&state, &[(key, Value::Bool(true))]).unwrap();
    session.restore_settings(&state).unwrap();
    assert!(!state.config().semantic_enabled());
}

#[test]
fn held_photo_keeps_metadata() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, _) = setup();
    let photo = crate::infrastructure::outdoors::items(&state.config()).remove(0);
    let held = output_status(&state, "*", photo.key.as_deref().unwrap(), &[], true);
    assert_eq!(held.name, photo.name.unwrap());
    assert_eq!(held.preview, photo.preview);
    assert!(held.held);
}

#[test]
fn held_library_keeps_metadata() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, _) = setup();
    let key = "static:held-library.png";
    state
        .with_db(|conn| {
            db::upsert_cache_entry(
                conn,
                key,
                "static",
                "held-library.png",
                "/held-thumb.webp",
                "/held-preview.webp",
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
    state.with_db(|conn| db::set_hidden(conn, key, true)).unwrap();
    let held = output_status(&state, "*", key, &[], true);
    assert_eq!(held.name, "held-library.png");
    assert_eq!(held.preview.as_deref(), Some("/held-thumb.webp"));
    assert!(held.held);
    let missing = output_status(&state, "*", "static:held-missing.png", &[], false);
    assert_eq!(missing.name, "held-missing");
    assert!(missing.preview.is_none());
    state.with_db(|conn| db::delete_entries(conn, &[key.to_owned()])).unwrap();
}

#[test]
fn next_preserves_other_displays() {
    let keys: Vec<_> = ["a", "b", "c", "d"].into_iter().map(str::to_owned).collect();
    let current = BTreeMap::from([
        ("left".into(), keys[0].clone()),
        ("center".into(), keys[1].clone()),
        ("right".into(), keys[2].clone()),
    ]);
    let index = next_index("left", &keys, 1, &current).unwrap();
    assert_eq!(keys[index], "d");
    assert_eq!(current["center"], "b");
    assert_eq!(current["right"], "c");
    assert!(next_index("left", &keys[..3], 1, &current).is_none());
}

#[test]
fn stop_restores_empty_desktop() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, ctx) = setup();
    skwd_wall_core::audio::write_state(&state.config().cache_dir(), &json!({}));
    follow(&ctx, true).unwrap();
    let key = crate::infrastructure::outdoors::items(&state.config()).remove(0).key.unwrap();
    let revision = {
        let mut guard = lock(&RUNTIME);
        let runtime = guard.as_mut().unwrap();
        runtime.plans.insert("*".into(), vec![key.clone()]);
        runtime.generation
    };
    assert!(apply(&ctx, &key, ApplySource::WeatherSemantic { revision }));
    skwd_wall_core::audio::write_state(
        &state.config().cache_dir(),
        &json!({"*":{"type":"static","path":key.strip_prefix("static:").unwrap()}}),
    );
    follow(&ctx, false).unwrap();
    assert!(current_keys(&state).is_empty());
    assert!(crate::infrastructure::persistence::last_any_source().is_none());
}

#[test]
fn empty_restore_preserves_manual() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, ctx) = setup();
    skwd_wall_core::audio::write_state(
        &state.config().cache_dir(),
        &json!({"left":{"type":"static","path":"/stock.jpg"},"right":{"type":"static","path":"/manual.jpg"}}),
    );
    let session = Session {
        owned: BTreeMap::from([
            ("left".into(), "static:/stock.jpg".into()),
            ("right".into(), "static:/stock.jpg".into()),
        ]),
        ..Session::default()
    };
    let _apply = state.apply().lock();
    session.release_empty(&ctx).unwrap();
    assert_eq!(
        current_keys(&state),
        BTreeMap::from([("right".into(), "static:/manual.jpg".into())])
    );
}

#[test]
fn winter_clouds_follow_ground() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, _) = setup();
    let context = Context { season: wall_proto::OutdoorSeason::Winter, ..summer() };
    let (bare, kind) = candidate_items(&state, &context, true);
    assert_eq!(kind, OutdoorMatch::Starter);
    assert!(bare.iter().any(|item| item.path.as_deref().unwrap().contains("winter_overcast_pond")));
    let snow = Context { snow: true, ..context };
    let (covered, _) = candidate_items(&state, &snow, true);
    assert!(
        covered.iter().all(|item| !item.path.as_deref().unwrap().contains("winter_overcast_pond"))
    );
}

#[test]
fn stale_freezes_automatic_changes() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, ctx) = setup();
    let items = crate::infrastructure::outdoors::items(&state.config());
    let key = items[0].key.as_ref().unwrap();
    let next = items[1].key.as_ref().unwrap();
    skwd_wall_core::audio::write_state(
        &state.config().cache_dir(),
        &json!({"*":{"type":"static","path":items[0].path}}),
    );
    let fresh = Forecast::default();
    let stale = Forecast { stale: true, ..fresh.clone() };
    let revision = {
        let mut guard = lock(&RUNTIME);
        let runtime = guard.as_mut().unwrap();
        runtime.context = Some(summer());
        reconcile(runtime, &state, &summer(), &fresh, items.clone(), OutdoorMatch::Starter);
        let revision = runtime.generation;
        accept_forecast(runtime, &state.config(), &stale);
        reconcile(runtime, &state, &summer(), &stale, items.clone(), OutdoorMatch::Starter);
        assert_ne!(runtime.generation, revision);
        assert_eq!(runtime.status.outdoors.as_ref().unwrap().outputs[0].key, *key);
        revision
    };
    assert!(assignment("*").is_none());
    assert!(!allows(&state, "*", revision, next));
    let stale_revision = lock(&RUNTIME).as_ref().unwrap().generation;
    assert!(!apply(&ctx, next, ApplySource::WeatherSemantic { revision: stale_revision }));
    assert_eq!(current_keys(&state)["*"], *key);
    assert!(feedback(&ctx, wall_proto::rpc::WALL_WEATHER_NEXT, "*", None).is_err());
    assert!(snapshot(&ctx).outdoors.unwrap().stale);
    {
        let mut guard = lock(&RUNTIME);
        let runtime = guard.as_mut().unwrap();
        accept_forecast(runtime, &state.config(), &fresh);
        reconcile(runtime, &state, &summer(), &fresh, items.clone(), OutdoorMatch::Starter);
    }
    let recovered = assignment("*").unwrap().1;
    assert_ne!(recovered, stale_revision);
    assert!(allows(&state, "*", recovered, next));
    assert!(apply(&ctx, next, ApplySource::WeatherSemantic { revision: recovered }));
}

#[test]
fn stale_keeps_cached_context() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, _) = setup();
    let mut runtime = load(&state);
    runtime.context = Some(summer());
    let stale = Forecast {
        stale: true,
        weather: vec!["cloudy".into()],
        observation: Some(Observation {
            latitude: -34.0,
            longitude: 18.0,
            observed_at: Some(1_781_524_800),
            ..Observation::default()
        }),
        ..Forecast::default()
    };
    accept_forecast(&mut runtime, &state.config(), &stale);
    assert_eq!(runtime.context, Some(summer()));
    assert!(runtime.pending.is_none());
    runtime.context = None;
    accept_forecast(&mut runtime, &state.config(), &stale);
    assert_eq!(runtime.context.unwrap().season, wall_proto::OutdoorSeason::Winter);
}

#[test]
fn follow_suspends_other_automation() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, ctx) = setup();
    crate::testenv::write_config(json!({"pickOnlyMode":true,"theme":{"policy":"off"}}));
    state.reload_config();
    follow(&ctx, true).unwrap();
    let key = crate::infrastructure::outdoors::items(&state.config()).remove(0).key.unwrap();
    for source in [ApplySource::Schedule, ApplySource::Rotation, ApplySource::Workspace] {
        assert!(!apply(&ctx, &key, source));
    }
    assert!(lock(&RUNTIME).as_ref().unwrap().session.held.is_empty());
    follow(&ctx, false).unwrap();
    assert!(apply(&ctx, &key, ApplySource::Schedule));
}

#[test]
fn unknown_weather_keeps_current() {
    use std::io::{Read, Write};
    let (_guard, _root) = crate::testenv::lock();
    let (state, ctx) = setup();
    skwd_wall_core::audio::write_state(
        &state.config().cache_dir(),
        &json!({"*":{"type":"static","path":"/manual.jpg"}}),
    );
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = [0; 4096];
        assert!(stream.read(&mut buffer).unwrap() > 0);
        let body = r#"{"current":{"weather_code":999,"time":1781524800}}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    let previous = std::env::var_os("SKWD_WEATHER_ENDPOINT");
    unsafe {
        std::env::set_var("SKWD_WEATHER_ENDPOINT", endpoint);
    }
    let status = refresh(&ctx, true);
    unsafe {
        if let Some(previous) = previous {
            std::env::set_var("SKWD_WEATHER_ENDPOINT", previous);
        } else {
            std::env::remove_var("SKWD_WEATHER_ENDPOINT");
        }
    }
    server.join().unwrap();
    assert_eq!(status.status, WeatherState::Unavailable);
    assert!(status.category.is_none());
    assert!(status.member_keys.is_empty());
    assert!(assignment("*").is_none());
    assert_eq!(current_keys(&state)["*"], "static:/manual.jpg");
    let guard = lock(&RUNTIME);
    let runtime = guard.as_ref().unwrap();
    assert!(runtime.forecast.observation.is_some());
    assert!(conditions(&runtime.forecast).is_none());
}

#[test]
fn retired_filter_stays_untouched() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, ctx) = setup();
    crate::testenv::write_config(json!({
        "pickOnlyMode":true,
        "weather":{"source":"semantic","enabled":false},
        "general":{"weatherMatch":true}
    }));
    state.reload_config();
    let captured = Session::capture(&state).unwrap();
    assert!(
        captured.settings.iter().all(|(key, _)| key != skwd_config::keys::general::WEATHER_MATCH)
    );
    assert!(
        captured
            .applied_settings
            .iter()
            .all(|(key, _)| key != skwd_config::keys::general::WEATHER_MATCH)
    );
    follow(&ctx, true).unwrap();
    follow(&ctx, false).unwrap();
    assert!(state.config().weather_match());
    let legacy = Session {
        settings: vec![(skwd_config::keys::general::WEATHER_MATCH.into(), json!(false))],
        applied_settings: vec![(skwd_config::keys::general::WEATHER_MATCH.into(), json!(true))],
        ..Session::default()
    };
    legacy.restore_settings(&state).unwrap();
    assert!(state.config().weather_match());
}

#[test]
fn legacy_selection_survives_restart() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, _) = setup();
    skwd_wall_core::audio::write_state(
        &state.config().cache_dir(),
        &json!({"*":{"type":"static","path":"/old.jpg"}}),
    );
    let mut runtime = load(&state);
    runtime.session.active = true;
    runtime.session.owned.insert("*".into(), "static:/old.jpg".into());
    let mut legacy = serde_json::to_value(&runtime.session).unwrap();
    legacy.as_object_mut().unwrap().remove("contexts");
    std::fs::write(&runtime.path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    let mut restored = load(&state);
    reconcile(
        &mut restored,
        &state,
        &summer(),
        &Forecast::default(),
        vec![item("static:/new.jpg")],
        OutdoorMatch::Library,
    );
    assert!(restored.due.is_empty());
    assert_eq!(restored.session.contexts["*"], summer());
    restored.session.contexts.clear();
    reconcile(
        &mut restored,
        &state,
        &summer(),
        &Forecast::default(),
        vec![item("static:/new.jpg")],
        OutdoorMatch::Library,
    );
    assert!(restored.due.contains("*"));
}

#[test]
fn changed_context_invalidates_plan() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, _) = setup();
    skwd_wall_core::audio::write_state(
        &state.config().cache_dir(),
        &json!({"*":{"type":"static","path":"/old.jpg"}}),
    );
    let mut runtime = load(&state);
    runtime.session.contexts.insert("*".into(), summer());
    reconcile(
        &mut runtime,
        &state,
        &summer(),
        &Forecast::default(),
        vec![item("static:/new.jpg")],
        OutdoorMatch::Library,
    );
    let revision = runtime.generation;
    let next = Context { category: "rainy".into(), ..summer() };
    reconcile(
        &mut runtime,
        &state,
        &next,
        &Forecast::default(),
        vec![item("static:/new.jpg")],
        OutdoorMatch::Library,
    );
    assert!(runtime.due.contains("*"));
    assert_ne!(runtime.generation, revision);
}

#[test]
fn accepted_context_invalidates_queue() {
    let (_guard, _root) = crate::testenv::lock();
    let (state, _) = setup();
    let mut runtime = load(&state);
    runtime.context = Some(summer());
    let forecast = Forecast {
        weather: vec!["cloudy".into()],
        observation: Some(Observation {
            latitude: -34.0,
            longitude: 18.0,
            observed_at: Some(1_781_524_800),
            ..Observation::default()
        }),
        ..Forecast::default()
    };
    let config = state.config().clone();
    accept_forecast(&mut runtime, &config, &forecast);
    let revision = runtime.generation;
    runtime.pending.as_mut().unwrap().1 =
        Instant::now().checked_sub(Duration::from_secs(config.outdoors_stable_seconds())).unwrap();
    accept_forecast(&mut runtime, &config, &forecast);
    assert_ne!(runtime.generation, revision);
    assert_eq!(runtime.context, context(&forecast));
}

#[test]
fn migration_requires_recorded_current() {
    let (_guard, _root) = crate::testenv::lock();
    for (legacy, initial, owned, due) in [
        (true, Some("static:/old.jpg"), None, false),
        (false, Some("static:/old.jpg"), None, true),
        (true, None, None, true),
        (true, Some("static:/old.jpg"), Some("static:/other.jpg"), true),
    ] {
        let (state, _) = setup();
        skwd_wall_core::audio::write_state(
            &state.config().cache_dir(),
            &json!({"*":{"type":"static","path":"/old.jpg"}}),
        );
        let mut runtime = load(&state);
        runtime.session.active = true;
        if let Some(key) = initial {
            runtime.session.wallpapers.insert("*".into(), key.into());
        }
        if let Some(key) = owned {
            runtime.session.owned.insert("*".into(), key.into());
        }
        let mut saved = serde_json::to_value(&runtime.session).unwrap();
        if legacy {
            saved.as_object_mut().unwrap().remove("contexts");
        }
        std::fs::write(&runtime.path, serde_json::to_vec(&saved).unwrap()).unwrap();
        let mut restored = load(&state);
        for _ in 0..2 {
            reconcile(
                &mut restored,
                &state,
                &summer(),
                &Forecast::default(),
                vec![item("static:/new.jpg")],
                OutdoorMatch::Library,
            );
            assert_eq!(restored.due.contains("*"), due);
        }
        assert_eq!(restored.session.contexts.get("*"), (!due).then_some(&summer()));
    }
}
