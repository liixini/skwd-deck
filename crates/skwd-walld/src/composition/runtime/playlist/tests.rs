#![cfg(test)]

use super::*;

fn out_state(keys: Vec<String>, dwell_secs: u64, next_fire: Instant) -> OutputState {
    OutputState {
        playlist_id: Some(1),
        weather_revision: None,
        keys,
        order: Order::Sequential,
        dwell: Duration::from_secs(dwell_secs),
        cursor: 0,
        next_fire,
        event_only: false,
        armed: true,
    }
}

#[test]
fn empty_due_no_spin() {
    let now = Instant::now();
    let mut rt = Runtime { outputs: HashMap::new(), rng: 1 };
    rt.outputs.insert("DP-1".into(), out_state(vec![], 600, now));
    let (applies, wait) = tick(&mut rt, now, Duration::from_secs(3600));
    assert!(applies.is_empty());
    assert!(wait >= Duration::from_secs(60));
    assert!(rt.outputs["DP-1"].next_fire > now);
}

#[test]
fn empty_retry_floor() {
    let now = Instant::now();
    let mut rt = Runtime { outputs: HashMap::new(), rng: 1 };
    rt.outputs.insert("DP-1".into(), out_state(vec![], 1, now));
    let (_, wait) = tick(&mut rt, now, Duration::from_secs(3600));
    assert!(wait >= Duration::from_secs(60));
}

#[test]
fn due_fires() {
    let now = Instant::now();
    let mut rt = Runtime { outputs: HashMap::new(), rng: 1 };
    rt.outputs.insert(
        "DP-1".into(),
        out_state(vec!["static:a.png".into(), "static:b.png".into()], 300, now),
    );
    let (applies, wait) = tick(&mut rt, now, Duration::from_secs(3600));
    assert_eq!(
        applies,
        vec![("DP-1".to_string(), "static:a.png".to_string(), ApplySource::Playlist)]
    );
    assert!(wait > Duration::from_secs(299) && wait <= Duration::from_secs(300));
}

#[test]
fn outdoors_stops_after_change() {
    let now = Instant::now();
    let mut rt = Runtime { outputs: HashMap::new(), rng: 1 };
    let mut output = out_state(vec!["static:a.png".into(), "static:b.png".into()], 0, now);
    output.playlist_id = None;
    output.weather_revision = Some(1);
    output.event_only = true;
    rt.outputs.insert("DP-1".into(), output);
    assert_eq!(tick(&mut rt, now, Duration::from_secs(3600)).0.len(), 1);
    assert!(tick(&mut rt, now + Duration::from_secs(1800), Duration::from_secs(3600)).0.is_empty());
    assert!(tick(&mut rt, now + Duration::from_secs(3600), Duration::from_secs(3600)).0.is_empty());
    queue_command(rt.outputs.get_mut("DP-1").unwrap(), true, &mut rt.rng, now);
    assert_eq!(tick(&mut rt, now, Duration::from_secs(3600)).0[0].1, "static:b.png");
}

#[test]
fn manual_step_no_skip() {
    let now = Instant::now();
    let mut rt = Runtime { outputs: HashMap::new(), rng: 1 };
    rt.outputs.insert(
        "DP-1".into(),
        out_state(
            vec!["static:a.png".into(), "static:b.png".into(), "static:c.png".into()],
            300,
            now + Duration::from_secs(300),
        ),
    );

    queue_command(rt.outputs.get_mut("DP-1").unwrap(), true, &mut rt.rng, now);
    assert_eq!(tick(&mut rt, now, Duration::from_secs(3600)).0[0].1, "static:a.png");

    queue_command(rt.outputs.get_mut("DP-1").unwrap(), true, &mut rt.rng, now);
    assert_eq!(tick(&mut rt, now, Duration::from_secs(3600)).0[0].1, "static:b.png");

    queue_command(rt.outputs.get_mut("DP-1").unwrap(), false, &mut rt.rng, now);
    assert_eq!(tick(&mut rt, now, Duration::from_secs(3600)).0[0].1, "static:a.png");
}

#[test]
fn not_due_waits() {
    let now = Instant::now();
    let mut rt = Runtime { outputs: HashMap::new(), rng: 1 };
    rt.outputs.insert(
        "DP-1".into(),
        out_state(vec!["static:a.png".into()], 300, now + Duration::from_secs(120)),
    );
    let (applies, wait) = tick(&mut rt, now, Duration::from_secs(3600));
    assert!(applies.is_empty());
    assert!(wait > Duration::from_secs(119) && wait <= Duration::from_secs(120));
}

#[tokio::test]
async fn queued_wake_skips_wait() {
    let engine =
        Engine { rt: Mutex::new(Runtime { outputs: HashMap::new(), rng: 1 }), wake: Notify::new() };
    engine.wake.notify_one();
    assert!(!wake_or_timeout(&engine.wake, Duration::from_secs(3600)).await);
    assert!(wake_or_timeout(&engine.wake, Duration::from_millis(15)).await);
}

#[test]
fn effective_precedence() {
    let assigns = vec![("*".to_string(), 1), ("DP-1".to_string(), 2)];
    assert_eq!(effective(&assigns, "DP-1"), Some(2));
    assert_eq!(effective(&assigns, "DP-2"), Some(1));
    let narrow = vec![("DP-1".to_string(), 3)];
    assert_eq!(effective(&narrow, "DP-2"), None);
}

#[test]
fn reconcile_keeps_cursor() {
    let (_g, _root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({}));
    let state = Arc::new(WallState::open().unwrap());
    let id = state.with_db(|conn| db::playlist_create(conn, "recon test")).unwrap();
    state
        .with_db(|conn| {
            db::playlist_update(conn, id, None, None, None, Some("sequential"), Some(30))
        })
        .unwrap();
    state.with_db(|conn| db::playlist_add_member(conn, id, "static:recon/r1.png")).unwrap();
    state.with_db(|conn| db::playlist_add_member(conn, id, "static:recon/r2.png")).unwrap();
    state.with_db(|conn| db::playlist_assign_set(conn, "*", Some(id))).unwrap();

    let mut rt = Runtime { outputs: HashMap::new(), rng: 1 };
    reconcile(&mut rt, &state);
    let st = rt.outputs.get("*").expect("wildcard assignment");
    assert_eq!(st.keys, vec!["static:recon/r1.png", "static:recon/r2.png"]);
    assert_eq!(st.order, Order::Sequential);
    assert_eq!(st.dwell, Duration::from_secs(30));
    assert_eq!(st.cursor, 0);
    let first_fire = st.next_fire;

    rt.outputs.get_mut("*").unwrap().cursor = 1;
    state.with_db(|conn| db::playlist_update(conn, id, None, None, None, None, Some(120))).unwrap();
    reconcile(&mut rt, &state);
    let st = &rt.outputs["*"];
    assert_eq!(st.cursor, 1);
    assert_eq!(st.dwell, Duration::from_secs(120));
    assert_eq!(st.next_fire, first_fire);

    state.with_db(|conn| db::playlist_remove_member(conn, id, "static:recon/r2.png")).unwrap();
    reconcile(&mut rt, &state);
    assert_eq!(rt.outputs["*"].cursor, 0);

    state.with_db(|conn| db::playlist_assign_clear(conn, Some(id))).unwrap();
    reconcile(&mut rt, &state);
    assert!(rt.outputs.is_empty());
    state.with_db(|conn| db::playlist_delete(conn, id)).unwrap();
}

#[test]
fn play_now_reconciles() {
    let (_g, _root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({}));
    let state = Arc::new(WallState::open().unwrap());
    let id = state.with_db(|conn| db::playlist_create(conn, "play now test")).unwrap();
    state.with_db(|conn| db::playlist_add_member(conn, id, "static:play-now/first.png")).unwrap();
    state.with_db(|conn| db::playlist_assign_set(conn, "*", Some(id))).unwrap();

    let now = Instant::now();
    let mut rt = Runtime { outputs: HashMap::new(), rng: 1 };
    assert!(command_runtime(&mut rt, &state, "*", true, now));
    let (applies, _) = tick(&mut rt, now, Duration::from_secs(3600));
    assert_eq!(
        applies,
        vec![("*".into(), "static:play-now/first.png".into(), ApplySource::Playlist)]
    );

    state.with_db(|conn| db::playlist_delete(conn, id)).unwrap();
}

#[test]
fn smart_source_keys() {
    let (_g, _root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({}));
    let state = Arc::new(WallState::open().unwrap());
    let id = state.with_db(|conn| db::playlist_create(conn, "recon smart")).unwrap();
    state
        .with_db(|conn| {
            db::playlist_update(
                conn,
                id,
                None,
                Some("smart"),
                Some("folder:reconsmart"),
                Some("sequential"),
                Some(30),
            )
        })
        .unwrap();
    for name in ["reconsmart/a.png", "other/b.png"] {
        state
            .with_db(|conn| {
                db::upsert_cache_entry(
                    conn,
                    &format!("static:{name}"),
                    "static",
                    name,
                    "",
                    "",
                    "",
                    "",
                    1,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                )
            })
            .unwrap();
    }
    state.with_db(|conn| db::playlist_assign_set(conn, "*", Some(id))).unwrap();
    let mut rt = Runtime { outputs: HashMap::new(), rng: 1 };
    reconcile(&mut rt, &state);
    assert_eq!(rt.outputs["*"].keys, vec!["static:reconsmart/a.png"]);
    state.with_db(|conn| db::playlist_delete(conn, id)).unwrap();
    state
        .with_db(|conn| {
            db::delete_entries(
                conn,
                &["static:reconsmart/a.png".to_string(), "static:other/b.png".to_string()],
            )
        })
        .unwrap();
}

#[test]
fn shuffled_first_selection_covers_all_members_and_survives_reload() {
    let (_g, _root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({}));
    let state = Arc::new(WallState::open().unwrap());
    let id = state.with_db(|conn| db::playlist_create(conn, "shuffle first")).unwrap();
    let keys: Vec<String> = (0..5).map(|i| format!("static:shuffle-first/{i}.png")).collect();
    for key in &keys {
        state.with_db(|conn| db::playlist_add_member(conn, id, key)).unwrap();
    }
    state.with_db(|conn| db::playlist_assign_set(conn, "*", Some(id))).unwrap();
    let mut firsts = std::collections::HashSet::new();
    for seed in 1..=128 {
        let mut rt = Runtime { outputs: HashMap::new(), rng: seed };
        reconcile(&mut rt, &state);
        let cursor = rt.outputs["*"].cursor;
        let rng = rt.rng;
        reconcile(&mut rt, &state);
        assert_eq!(rt.outputs["*"].cursor, cursor);
        assert_eq!(rt.rng, rng);
        let now = rt.outputs["*"].next_fire;
        if seed % 2 == 0 {
            assert!(command_runtime(&mut rt, &state, "*", true, now));
        }
        let first = tick(&mut rt, now, Duration::from_secs(3600)).0[0].1.clone();
        firsts.insert(first.clone());
        assert!(command_runtime(&mut rt, &state, "*", true, now));
        let second = tick(&mut rt, now, Duration::from_secs(3600)).0[0].1.clone();
        assert_ne!(first, second);
    }
    state.with_db(|conn| db::playlist_delete(conn, id)).unwrap();
    assert_eq!(firsts, keys.into_iter().collect());
}

#[test]
fn shuffled_empty_playlist_randomizes_when_members_arrive() {
    let (_g, _root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({}));
    let state = Arc::new(WallState::open().unwrap());
    let id = state.with_db(|conn| db::playlist_create(conn, "shuffle empty")).unwrap();
    state.with_db(|conn| db::playlist_assign_set(conn, "*", Some(id))).unwrap();
    let keys = ["static:shuffle-empty/a.png", "static:shuffle-empty/b.png"];
    let mut firsts = std::collections::HashSet::new();
    for seed in 1..=32 {
        let mut rt = Runtime { outputs: HashMap::new(), rng: seed };
        reconcile(&mut rt, &state);
        let now = rt.outputs["*"].next_fire;
        assert!(tick(&mut rt, now, Duration::from_secs(3600)).0.is_empty());
        for key in keys {
            state.with_db(|conn| db::playlist_add_member(conn, id, key)).unwrap();
        }
        assert!(command_runtime(&mut rt, &state, "*", true, now));
        firsts.insert(tick(&mut rt, now, Duration::from_secs(3600)).0[0].1.clone());
        for key in keys {
            state.with_db(|conn| db::playlist_remove_member(conn, id, key)).unwrap();
        }
    }
    state.with_db(|conn| db::playlist_delete(conn, id)).unwrap();
    assert_eq!(firsts, keys.into_iter().map(str::to_string).collect());
}

#[test]
fn counts_follow_profile() {
    let (_g, root) = crate::testenv::lock();
    let anime = root.join("walls/anime").to_string_lossy().into_owned();
    crate::testenv::write_config(serde_json::json!({
        "library": {"profile": "public", "profiles": [{"id": "public", "hide": [anime]}]}
    }));
    let state = Arc::new(WallState::open().unwrap());
    let insert = |key: &str, name: &str| {
        state
            .with_db(|conn| {
                db::upsert_cache_entry(
                    conn, key, "static", name, "/t.webp", "/s.webp", "", "", 1, 0, 0, 0, 1, 1, 1,
                )
            })
            .unwrap();
    };
    insert("static:a.png", "a.png");
    insert("static:anime/b.png", "anime/b.png");
    insert("static:c.png", "c.png");
    state.with_db(|conn| db::set_hidden(conn, "static:c.png", true)).unwrap();
    let id = state.with_db(|conn| db::playlist_create(conn, "profile count")).unwrap();
    state
        .with_db(|conn| db::playlist_update(conn, id, None, Some("smart"), Some("all"), None, None))
        .unwrap();
    let rows = list_with_resolved_counts(&state);
    let row = rows.iter().find(|row| row.id == id).expect("smart row");
    let defs = load_defs(&state);
    let keys = resolve_keys(&state, id, &defs[&id]);
    assert!(keys.iter().any(|key| key == "static:a.png"));
    assert!(!keys.iter().any(|key| key == "static:anime/b.png" || key == "static:c.png"));
    assert_eq!(usize::try_from(row.count).ok(), Some(keys.len()));
    for key in ["static:a.png", "static:anime/b.png", "static:c.png"] {
        state.with_db(|conn| db::delete_entries(conn, &[key.to_string()])).unwrap();
    }
    state.with_db(|conn| db::playlist_delete(conn, id)).unwrap();
}

#[test]
fn counts_mixed_sources() {
    let (_guard, root) = crate::testenv::lock();
    let anime = root.join("walls/counts-mixed/anime").to_string_lossy().into_owned();
    let games = root.join("walls/counts-mixed/games").to_string_lossy().into_owned();
    crate::testenv::write_config(serde_json::json!({
        "library": {"profile": "public", "profiles": [{"id": "public", "hide": [anime]}]}
    }));
    let state = Arc::new(WallState::open().unwrap());
    for (name, fav, hidden, excluded) in [
        ("games/a.png", 1, 0, 0),
        ("games/b.png", 0, 0, 0),
        ("anime/c.png", 1, 0, 0),
        ("games/d.png", 1, 1, 0),
        ("games/e.png", 1, 0, 1),
        ("other/f.png", 1, 0, 0),
    ] {
        state
            .with_db(|conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO meta(key,name,type,favourite,hidden,library_excluded) VALUES (?1,?2,'static',?3,?4,?5)",
                    (format!("static:counts-mixed/{name}"), format!("counts-mixed/{name}"), fav, hidden, excluded),
                )
            })
            .unwrap();
    }
    state.with_db(|conn| db::set_effect_tag(conn, "counts-mixed/games/b", "glow")).unwrap();

    let sources = [
        ("all folder:counts-mixed".to_owned(), 3),
        ("favourites folder:counts-mixed".to_owned(), 2),
        ("tag:glow folder:counts-mixed".to_owned(), 1),
        (wall_proto::playlist_source::folder_clause(&games), 2),
    ];
    let ids: Vec<_> = sources
        .iter()
        .map(|(source, _)| {
            let id = state.with_db(|conn| db::playlist_create(conn, source)).unwrap();
            state
                .with_db(|conn| {
                    db::playlist_update(
                        conn,
                        id,
                        None,
                        Some("smart"),
                        Some(source.as_str()),
                        None,
                        None,
                    )
                })
                .unwrap();
            id
        })
        .collect();
    let curated = state.with_db(|conn| db::playlist_create(conn, "curated")).unwrap();
    state
        .with_db(|conn| db::playlist_add_member(conn, curated, "static:counts-mixed/games/a.png"))
        .unwrap();

    let rows = list_with_resolved_counts(&state);
    let defs = load_defs(&state);
    for ((_, expected), id) in sources.iter().zip(&ids) {
        let row = rows.iter().find(|row| row.id == *id).unwrap();
        assert_eq!(row.count, *expected);
        assert_eq!(usize::try_from(row.count).unwrap(), resolve_keys(&state, *id, &defs[id]).len());
    }
    assert_eq!(rows.iter().find(|row| row.id == curated).unwrap().count, 1);

    for id in &ids {
        state
            .with_db(|conn| {
                db::playlist_update(
                    conn,
                    *id,
                    None,
                    None,
                    Some("favourites folder:counts-mixed"),
                    None,
                    None,
                )
            })
            .unwrap();
    }
    let rows = list_with_resolved_counts(&state);
    assert!(rows.iter().filter(|row| ids.contains(&row.id)).all(|row| row.count == 2));
    state.with_db(|conn| db::playlist_delete(conn, curated)).unwrap();
    for id in ids {
        state.with_db(|conn| db::playlist_delete(conn, id)).unwrap();
    }
    state
        .with_db(|conn| {
            db::delete_entries(
                conn,
                &[
                    "static:counts-mixed/games/a.png".to_owned(),
                    "static:counts-mixed/games/b.png".to_owned(),
                    "static:counts-mixed/anime/c.png".to_owned(),
                    "static:counts-mixed/games/d.png".to_owned(),
                    "static:counts-mixed/games/e.png".to_owned(),
                    "static:counts-mixed/other/f.png".to_owned(),
                ],
            )
        })
        .unwrap();
}

#[test]
fn folder_playlists_nest() {
    let (_g, root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({}));
    let games = root.join("walls/games").to_string_lossy().into_owned();
    let state = Arc::new(WallState::open().unwrap());
    let keys = ["static:games/mh/a.png", "static:games/b.png", "static:anime/c.png"];
    for key in keys {
        let name = key.trim_start_matches("static:");
        state
            .with_db(|conn| {
                db::upsert_cache_entry(
                    conn, key, "static", name, "/t.webp", "/s.webp", "", "", 1, 0, 0, 0, 1, 1, 1,
                )
            })
            .unwrap();
    }
    let source = wall_proto::playlist_source::folder_clause(&games);
    let id = state.with_db(|conn| db::playlist_create(conn, "games")).unwrap();
    state
        .with_db(|conn| {
            db::playlist_update(conn, id, None, Some("smart"), Some(&source), None, None)
        })
        .unwrap();
    let defs = load_defs(&state);
    let resolved = resolve_keys(&state, id, &defs[&id]);
    assert!(resolved.iter().any(|key| key == "static:games/mh/a.png"));
    assert!(resolved.iter().any(|key| key == "static:games/b.png"));
    assert!(!resolved.iter().any(|key| key == "static:anime/c.png"));
    for key in keys {
        state.with_db(|conn| db::delete_entries(conn, &[key.to_string()])).unwrap();
    }
    state.with_db(|conn| db::playlist_delete(conn, id)).unwrap();
}

#[test]
fn weather_starts_immediately() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({"weather":{"enabled":true}}));
    let state = Arc::new(WallState::open().unwrap());
    let id = state.with_db(|conn| db::playlist_create(conn, "weather cycle")).unwrap();
    state
        .with_db(|conn| {
            db::playlist_update(conn, id, None, None, None, Some("sequential"), Some(30))
        })
        .unwrap();
    state.with_db(|conn| db::playlist_add_member(conn, id, "static:weather/first.png")).unwrap();
    state.with_db(|conn| db::playlist_add_member(conn, id, "static:weather/second.png")).unwrap();
    state.with_db(|conn| db::playlist_assign_clear(conn, None)).unwrap();
    super::super::weather::test_status(
        &state,
        Some(wall_proto::WeatherStatus {
            enabled: true,
            playlist_id: Some(id),
            status: wall_proto::WeatherState::Empty,
            ..wall_proto::WeatherStatus::default()
        }),
    );
    let mut runtime = Runtime { outputs: HashMap::new(), rng: 1 };
    reconcile(&mut runtime, &state);
    assert!(tick(&mut runtime, Instant::now(), Duration::from_secs(3600)).0.is_empty());
    crate::testenv::cache_item(&state, "weather/first.png");
    crate::testenv::cache_item(&state, "weather/second.png");
    super::super::weather::test_status(
        &state,
        Some(wall_proto::WeatherStatus {
            enabled: true,
            playlist_id: Some(id),
            member_keys: vec![
                "static:weather/first.png".into(),
                "static:weather/second.png".into(),
            ],
            status: wall_proto::WeatherState::Active,
            ..wall_proto::WeatherStatus::default()
        }),
    );
    reconcile(&mut runtime, &state);
    let now = Instant::now();
    assert_eq!(
        tick(&mut runtime, now, Duration::from_secs(3600)).0[0].1,
        "static:weather/first.png"
    );
    reconcile(&mut runtime, &state);
    assert!(tick(&mut runtime, now, Duration::from_secs(3600)).0.is_empty());
    assert_eq!(
        tick(&mut runtime, now + Duration::from_secs(30), Duration::from_secs(3600)).0[0].1,
        "static:weather/second.png"
    );
    assert!(state.with_db(db::playlist_assigns).unwrap().is_empty());
    super::super::weather::test_status(&state, None);
    reconcile(&mut runtime, &state);
    assert!(runtime.outputs.is_empty());
    state.with_db(|conn| db::playlist_delete(conn, id)).unwrap();
    for name in ["weather/first.png", "weather/second.png"] {
        state.with_db(|conn| db::delete_by_name(conn, name)).unwrap();
    }
}

#[test]
fn manual_assignment_wins_weather() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({"weather":{"enabled":true}}));
    let state = Arc::new(WallState::open().unwrap());
    let id = state.with_db(|conn| db::playlist_create(conn, "manual cycle")).unwrap();
    state.with_db(|conn| db::playlist_assign_set(conn, "*", Some(id))).unwrap();
    super::super::weather::test_status(
        &state,
        Some(wall_proto::WeatherStatus {
            enabled: true,
            playlist_id: Some(id + 1),
            member_keys: vec!["static:weather/first.png".into()],
            status: wall_proto::WeatherState::Active,
            ..wall_proto::WeatherStatus::default()
        }),
    );
    let mut runtime = Runtime { outputs: HashMap::new(), rng: 1 };
    reconcile(&mut runtime, &state);
    assert_eq!(runtime.outputs["*"].playlist_id, Some(id));
    assert!(runtime.outputs["*"].weather_revision.is_none());
    assert!(runtime.outputs["*"].next_fire > Instant::now());
    super::super::weather::test_status(&state, None);
    state.with_db(|conn| db::playlist_delete(conn, id)).unwrap();
}

#[test]
fn semantic_weather_rotates() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(
        serde_json::json!({"weather":{"enabled":true,"source":"semantic","semantic":{"dwellSeconds":42}}}),
    );
    let state = Arc::new(WallState::open().unwrap());
    super::super::weather::test_status(
        &state,
        Some(wall_proto::WeatherStatus {
            enabled: true,
            source: wall_proto::WeatherSource::Semantic,
            category: Some("clear".into()),
            query: Some("clear blue sky and sunshine".into()),
            member_keys: vec!["static:semantic-a.png".into(), "static:semantic-b.png".into()],
            status: wall_proto::WeatherState::Active,
            ..wall_proto::WeatherStatus::default()
        }),
    );
    let mut runtime = Runtime { outputs: HashMap::new(), rng: 1 };
    reconcile(&mut runtime, &state);
    let output = &runtime.outputs["*"];
    assert_eq!(output.playlist_id, None);
    assert_eq!(output.dwell, Duration::from_secs(42));
    let now = Instant::now();
    let (applies, _) = tick(&mut runtime, now, Duration::from_secs(3600));
    assert_eq!(applies.len(), 1);
    assert_eq!(applies[0].2, ApplySource::WeatherSemantic { revision: 1 });
    super::super::weather::test_status(&state, None);
}

#[test]
fn weather_failure_retries_later() {
    let now = Instant::now();
    let mut output = out_state(vec!["static:a.png".into()], 0, now);
    output.playlist_id = None;
    output.weather_revision = Some(1);
    output.event_only = true;
    let mut rt = Runtime { outputs: HashMap::from([("DP-1".into(), output)]), rng: 1 };
    let long = Duration::from_secs(3600);
    assert_eq!(tick(&mut rt, now, long).0.len(), 1);
    assert!(rt.outputs.get_mut("DP-1").unwrap().retry_weather(1, now));
    assert!(tick(&mut rt, now, long).0.is_empty());
    assert_eq!(tick(&mut rt, now + MIN_DWELL, long).0.len(), 1);
    assert!(tick(&mut rt, now + long, long).0.is_empty());
    assert!(!rt.outputs.get_mut("DP-1").unwrap().retry_weather(2, now));
    assert!(!rt.outputs["DP-1"].armed);
}
