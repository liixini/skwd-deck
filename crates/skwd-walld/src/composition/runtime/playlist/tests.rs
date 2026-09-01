#![cfg(test)]

use super::*;

fn out_state(keys: Vec<String>, dwell_secs: u64, next_fire: Instant) -> OutputState {
    OutputState {
        playlist_id: 1,
        keys,
        order: Order::Sequential,
        dwell: Duration::from_secs(dwell_secs),
        cursor: 0,
        next_fire,
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
    assert_eq!(applies, vec![("DP-1".to_string(), "static:a.png".to_string())]);
    assert!(wait > Duration::from_secs(299) && wait <= Duration::from_secs(300));
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
    assert_eq!(applies, vec![("*".into(), "static:play-now/first.png".into())]);

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
