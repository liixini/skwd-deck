#![cfg(test)]

use crate::backend::history::ApplySource;
use crate::composition::apply::{apply_core, split_locked_outputs, stage_unlocked_media_outputs};
use crate::testenv;
use crate::testenv::{call, ecode, events, harness, rr, subscribe};
use serde_json::{Value, json};

#[test]
fn pick_only_apply() {
    let (_g, root) = testenv::lock();
    testenv::write_config(json!({"pickOnlyMode": true}));
    let wall = root.join("walls/pick-only-test.png");
    std::fs::write(&wall, b"png").unwrap();
    let path = wall.to_str().unwrap().to_string();
    let (state, subs, stats) = harness();
    state
        .with_db(|connection| {
            skwd_wall_core::db::upsert_cache_entry(
                connection,
                "static:pick-only-test.png",
                wall_proto::kind::STATIC,
                "pick-only-test.png",
                "/thumb.webp",
                "/thumb-sm.webp",
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
    let pause_output = root.join("pick-only-pause.stdin");
    let mut child = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("exec cat > '{}'", pause_output.display()))
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let stdin = child.stdin.take();
    state.renderers().set_video_paper("DP-1", child, stdin);
    state.renderers().set_session_paused(17, true);
    let mut rx = subscribe(&subs);
    let v = rr(call(
        &state,
        &subs,
        &stats,
        "wall.apply",
        json!({"type": "static", "path": path, "mute": true, "volume": 0, "notify": false}),
    ));
    state.renderers().set_session_paused(17, false);
    for (mut child, stdin) in state.renderers().take_all_video_papers() {
        drop(stdin);
        let _ = child.wait();
    }
    assert_eq!(v["applied"], json!(path));
    assert!(!state.renderers().renderer_alive("static"));
    assert_eq!(
        std::fs::read_to_string(pause_output).unwrap(),
        "{\"to\":\"\",\"pause\":true}\n{\"to\":\"\",\"pause\":false}\n{\"to\":\"\",\"pause\":true}\n{\"to\":\"\",\"pause\":false}\n"
    );
    let last: Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("cache/skwd-wall-v2/last-wallpaper.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(last["type"], json!("static"));
    assert_eq!(last["path"], json!(path));
    assert_eq!(last["thumb"], json!(path));
    let evs = events(&mut rx);
    let applied = evs
        .iter()
        .find(|e| e.event == "skwd.wall.applied")
        .expect("pick-only apply must still broadcast skwd.wall.applied");
    assert_eq!(applied.data["path"], json!(path));
    assert_eq!(applied.data["key"], "static:pick-only-test.png");
    assert_eq!(applied.data["type"], json!("static"));
    let rows =
        state.with_db(|connection| skwd_wall_core::db::list_wallpapers(connection, false)).unwrap();
    let row = rows
        .iter()
        .find(|row| row["key"] == "static:pick-only-test.png")
        .expect("applied library row remains present");
    assert_eq!(row["apply_count"], 1);
    assert!(row["last_applied"].as_i64().is_some_and(|rank| rank > 0));
    assert!(evs.iter().any(|e| e.event == "skwd.wall.apply_result"));
}

#[test]
fn automatic_and_regular_user_applies_skip_a_locked_output() {
    let (_guard, root) = testenv::lock();
    testenv::write_config(json!({
        "pickOnlyMode": true,
        "display": {"outputLocks": {"DP-1": true}}
    }));
    let wall = root.join("walls/locked-test.png");
    std::fs::write(&wall, b"png").unwrap();
    let path = wall.to_string_lossy().into_owned();
    let (state, subscribers, stats) = harness();

    let skipped = rr(call(
        &state,
        &subscribers,
        &stats,
        "wall.apply",
        json!({
            "type": "static", "path": path, "output": "DP-1",
            "source": "random", "notify": false
        }),
    ));
    assert_eq!(skipped["locked"], "DP-1");

    let user = rr(call(
        &state,
        &subscribers,
        &stats,
        "wall.apply",
        json!({"type": "static", "path": path, "output": "DP-1", "notify": false}),
    ));
    assert_eq!(user["locked"], "DP-1");

    let override_apply = rr(call(
        &state,
        &subscribers,
        &stats,
        "wall.apply",
        json!({
            "type": "static", "path": path, "output": "DP-1", "notify": false,
            "override_locks": true
        }),
    ));
    assert_eq!(override_apply["applied"], path);
    assert!(state.config().display().output_locked("DP-1"));
}

#[test]
fn automatic_global_apply_targets_only_unlocked_live_outputs() {
    let (unlocked, preserved) = split_locked_outputs(
        vec![String::from("DP-1"), String::from("DP-2"), String::from("eDP-1")],
        &[String::from("DP-2"), String::from("disconnected")],
    );
    assert_eq!(unlocked, ["DP-1", "eDP-1"]);
    assert_eq!(preserved, ["DP-2"]);
}

#[test]
fn locked_global_video_stages_every_unlocked_output_before_reconcile() {
    let (_guard, _root) = testenv::lock();
    testenv::write_config(json!({}));
    let (state, _subscribers, _stats) = harness();
    let cache = state.config().cache_dir();
    skwd_wall_core::audio::set_entry(
        &cache,
        "*",
        wall_proto::kind::VIDEO,
        "/wall/old.mp4",
        "",
        true,
        80,
    );
    skwd_wall_core::audio::set_entry(
        &cache,
        "DP-2",
        wall_proto::kind::STATIC,
        "/wall/locked.png",
        "",
        true,
        0,
    );

    stage_unlocked_media_outputs(
        &state,
        &[String::from("DP-1"), String::from("DP-3")],
        wall_proto::kind::VIDEO,
        "/wall/new.mp4",
        "",
        false,
        65,
    );

    let staged = skwd_wall_core::audio::read_state(&cache);
    assert_eq!(staged["DP-1"]["path"], "/wall/new.mp4");
    assert_eq!(staged["DP-3"]["path"], "/wall/new.mp4");
    assert_eq!(staged["DP-1"]["mute"], false);
    assert_eq!(staged["DP-3"]["volume"], 65);
    assert_eq!(staged["DP-2"]["path"], "/wall/locked.png");
}

#[test]
fn history_back_forward() {
    let (_g, root) = testenv::lock();
    testenv::write_config(json!({"pickOnlyMode": true}));
    let cache = root.join("cache/skwd-wall-v2");
    std::fs::create_dir_all(&cache).unwrap();
    for f in ["history.json", "last-wallpaper.json", "outputs.json"] {
        let _ = std::fs::remove_file(cache.join(f));
    }
    let mk = |name: &str| {
        let p = root.join("walls").join(name);
        std::fs::write(&p, b"png").unwrap();
        p.to_str().unwrap().to_string()
    };
    let a = mk("hist-a.png");
    let b = mk("hist-b.png");
    let c = mk("hist-c.png");
    let (state, subs, stats) = harness();
    let last = || {
        let v: Value = serde_json::from_str(
            &std::fs::read_to_string(cache.join("last-wallpaper.json")).unwrap(),
        )
        .unwrap();
        v["path"].as_str().unwrap().to_string()
    };
    let apply_user = |path: &str| {
        rr(call(
            &state,
            &subs,
            &stats,
            "wall.apply",
            json!({"type": "static", "path": path, "mute": true, "volume": 0, "notify": false}),
        ));
    };
    apply_user(&a);
    apply_user(&b);
    assert_eq!(last(), b);

    let back = rr(call(&state, &subs, &stats, "wall.history.back", json!({})));
    assert_eq!(back["ok"], json!(true));
    assert_eq!(last(), a);
    let listed = rr(call(&state, &subs, &stats, "wall.history.list", json!({})));
    assert_eq!(listed["outputs"]["*"]["pos"], json!(0), "replay must not re-record");
    assert_eq!(listed["outputs"]["*"]["entries"].as_array().unwrap().len(), 2);

    let fwd = rr(call(&state, &subs, &stats, "wall.history.forward", json!({})));
    assert_eq!(fwd["ok"], json!(true));
    assert_eq!(last(), b);

    let application = skwd_wall_core::infrastructure::wallpaper::CoreWallpaperApplication::new(
        std::sync::Arc::clone(&state),
    );
    let history =
        crate::infrastructure::history::FileHistoryRepository::new(state.config().cache_dir());
    let res = apply_core(
        &state,
        &application,
        &history,
        subs.as_ref(),
        &stats,
        "static",
        &c,
        "",
        true,
        0,
        ApplySource::Rotation,
        "*",
        false,
        false,
        None,
        None,
    );
    assert!(res.is_ok());
    assert_eq!(last(), c);
    let listed = rr(call(&state, &subs, &stats, "wall.history.list", json!({})));
    assert_eq!(listed["outputs"]["*"]["entries"].as_array().unwrap().len(), 2);
    let back = rr(call(&state, &subs, &stats, "wall.history.back", json!({})));
    assert_eq!(back["ok"], json!(true));
    assert_eq!(last(), a);
}

#[test]
fn history_back_empty() {
    let (_g, root) = testenv::lock();
    testenv::write_config(json!({"pickOnlyMode": true}));
    let cache = root.join("cache/skwd-wall-v2");
    std::fs::create_dir_all(&cache).unwrap();
    for f in ["history.json", "outputs.json"] {
        let _ = std::fs::remove_file(cache.join(f));
    }
    let (state, subs, stats) = harness();
    let r = rr(call(&state, &subs, &stats, "wall.history.back", json!({"output": "DP-9"})));
    assert_eq!(r["ok"], json!(false));
    assert!(r["message"].as_str().unwrap().contains("no back history"));
}

#[test]
fn history_disabled_noop() {
    let (_g, _root) = testenv::lock();
    testenv::write_config(json!({"history": {"enabled": false}}));
    let (state, subs, stats) = harness();
    let r = rr(call(&state, &subs, &stats, "wall.history.back", json!({})));
    assert_eq!(r["ok"], json!(false));
    assert!(r["message"].as_str().unwrap().contains("disabled"));
}

#[test]
fn reapply_live_noop() {
    let (_g, root) = testenv::lock();
    testenv::write_config(json!({}));
    let wall = root.join("walls/noop-test.png");
    std::fs::write(&wall, b"png").unwrap();
    let path = wall.to_str().unwrap().to_string();
    std::fs::create_dir_all(root.join("cache/skwd-wall-v2")).unwrap();
    std::fs::write(
            root.join("cache/skwd-wall-v2/last-wallpaper.json"),
            json!({"type": "static", "path": path, "we_id": "", "mute": true, "volume": 0, "thumb": path})
                .to_string(),
        )
        .unwrap();
    std::fs::write(
        root.join("cache/skwd-wall-v2/outputs.json"),
        json!({"*": {"type": "static", "path": path, "we_id": "", "mute": true, "volume": 0}})
            .to_string(),
    )
    .unwrap();
    let (state, subs, stats) = harness();
    let child = std::process::Command::new("sleep").arg("30").spawn().unwrap();
    state.renderers().set_base_still(child, None);
    state.apply().set_render_fill("fill");
    let r = call(
        &state,
        &subs,
        &stats,
        "wall.apply",
        json!({"type": "static", "path": path, "mute": true, "volume": 0, "notify": false, "no_transition": true}),
    );
    state.renderers().kill_base_still();
    let v = rr(r);
    assert_eq!(v["noop"], json!(true));
    assert_eq!(v["applied"], json!(path));
    assert!(!state.apply().no_transition());
}

#[test]
fn changing_the_fill_mode_breaks_the_reapply_noop() {
    let (_g, root) = testenv::lock();
    testenv::write_config(json!({"display": {"fillMode": "center"}}));
    let wall = root.join("walls/fill-test.png");
    std::fs::write(&wall, b"png").unwrap();
    let path = wall.to_str().unwrap().to_string();
    std::fs::create_dir_all(root.join("cache/skwd-wall-v2")).unwrap();
    std::fs::write(
            root.join("cache/skwd-wall-v2/last-wallpaper.json"),
            json!({"type": "static", "path": path, "we_id": "", "mute": true, "volume": 0, "thumb": path})
                .to_string(),
        )
        .unwrap();
    std::fs::write(
        root.join("cache/skwd-wall-v2/outputs.json"),
        json!({"*": {"type": "static", "path": path, "we_id": "", "mute": true, "volume": 0}})
            .to_string(),
    )
    .unwrap();
    let (state, subs, stats) = harness();
    let child = std::process::Command::new("sleep").arg("30").spawn().unwrap();
    state.renderers().set_base_still(child, None);
    state.apply().set_render_fill("fill");
    let r = call(
        &state,
        &subs,
        &stats,
        "wall.apply",
        json!({"type": "static", "path": path, "mute": true, "volume": 0, "notify": false}),
    );
    state.renderers().kill_base_still();
    assert_ne!(
        r.result.as_ref().and_then(|v| v.get("noop").cloned()),
        Some(json!(true)),
        "the same wallpaper under a new fill mode must re-render, not report noop"
    );
}

#[test]
fn reapply_star_reconciles_when_per_output_diverged() {
    let (_g, root) = testenv::lock();
    testenv::write_config(json!({}));
    let wall = root.join("walls/diverge-test.png");
    std::fs::write(&wall, b"png").unwrap();
    let path = wall.to_str().unwrap().to_string();
    std::fs::create_dir_all(root.join("cache/skwd-wall-v2")).unwrap();
    std::fs::write(
            root.join("cache/skwd-wall-v2/last-wallpaper.json"),
            json!({"type": "static", "path": path, "we_id": "", "mute": true, "volume": 0, "thumb": path})
                .to_string(),
        )
        .unwrap();
    std::fs::write(
        root.join("cache/skwd-wall-v2/outputs.json"),
        json!({"*": {"type": "video", "path": "/other.mp4", "we_id": "", "mute": false, "volume": 50}})
            .to_string(),
    )
    .unwrap();
    let (state, subs, stats) = harness();
    let child = std::process::Command::new("sleep").arg("30").spawn().unwrap();
    state.renderers().set_base_still(child, None);
    let r = call(
        &state,
        &subs,
        &stats,
        "wall.apply",
        json!({"type": "static", "path": path, "mute": true, "volume": 0, "notify": false}),
    );
    state.renderers().kill_base_still();
    if let Some(v) = r.result.as_ref() {
        assert_ne!(v["noop"], json!(true));
    }
    assert_eq!(
        ecode(&r),
        -1,
        "diverged outputs.json must reconcile (hitting the fake renderer), not no-op"
    );
}

#[test]
fn reapply_dead_renderer() {
    let (_g, root) = testenv::lock();
    testenv::write_config(json!({}));
    let wall = root.join("walls/dead-renderer-test.png");
    std::fs::write(&wall, b"png").unwrap();
    let path = wall.to_str().unwrap().to_string();
    std::fs::create_dir_all(root.join("cache/skwd-wall-v2")).unwrap();
    std::fs::write(
            root.join("cache/skwd-wall-v2/last-wallpaper.json"),
            json!({"type": "static", "path": path, "we_id": "", "mute": true, "volume": 0, "thumb": path})
                .to_string(),
        )
        .unwrap();
    let (state, subs, stats) = harness();
    let r = call(
        &state,
        &subs,
        &stats,
        "wall.apply",
        json!({"type": "static", "path": path, "mute": true, "volume": 0, "notify": false}),
    );
    if let Some(v) = r.result.as_ref() {
        assert_ne!(v["noop"], json!(true));
    }
    assert_eq!(ecode(&r), -1);
}
