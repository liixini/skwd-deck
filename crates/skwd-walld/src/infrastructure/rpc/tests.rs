#![cfg(test)]

use super::{classify_apply_error, peer_uid_allowed, status_payload};
use crate::testenv::{self, call, ecode, emsg, events, harness, rr, subscribe};
use serde_json::{Value, json};
use skwd_wall_core::db;
use wall_proto::Response;

#[test]
fn preview_fetches_coalesce() {
    let key = "preview:/tmp/skwd-test/remote/wallhaven/abc.jpg";
    assert!(super::dl_inflight_begin(key));
    assert!(!super::dl_inflight_begin(key));
    super::dl_inflight_end(key);
    assert!(super::dl_inflight_begin(key));
    super::dl_inflight_end(key);
}

#[test]
fn all_sources_route() {
    let _guard = testenv::lock();
    testenv::write_config(json!({
        "sources": {
            "unsplash": { "enabled": true }, "pexels": { "enabled": true },
            "youtube": { "enabled": true }, "bing": { "enabled": true },
        }
    }));
    let (state, subs, stats) = harness();
    for spec in wall_proto::sources::SOURCES {
        for verb in ["list", "preview", "download"] {
            let resp = call(
                &state,
                &subs,
                &stats,
                &format!("source.{verb}"),
                json!({"source": spec.key, "id": "x", "full_url": ""}),
            );
            let msg = resp.error.as_ref().map(|ev| ev.message.clone()).unwrap_or_default();
            assert!(!msg.starts_with("unknown source"), "{}.{verb} unrouted", spec.key);
        }
    }
}

#[test]
fn apply_error_buckets() {
    assert_eq!(classify_apply_error("missing path"), "bad_request");
    assert_eq!(classify_apply_error("No such file or directory"), "file_missing");
    assert_eq!(classify_apply_error("ffmpeg decode failed"), "decode_failed");
    assert_eq!(
        classify_apply_error(
            "Paper renderer_unavailable: static image media capability requires skwd-wall-still"
        ),
        "renderer_unavailable"
    );
    assert_eq!(classify_apply_error("failed to spawn skwd-wall-vk"), "renderer_spawn_failed");
    assert_eq!(classify_apply_error("something weird"), "apply_failed");
}

#[test]
fn peer_uid_gate() {
    assert!(peer_uid_allowed(Some(1000), 1000));
    assert!(!peer_uid_allowed(Some(0), 1000));
    assert!(!peer_uid_allowed(Some(1001), 1000));
    assert!(!peer_uid_allowed(None, 1000));
}

#[test]
fn unknown_method_32601() {
    let (_guard, _root) = testenv::lock();
    testenv::write_config(json!({}));
    let (state, subs, stats) = harness();
    let resp = call(&state, &subs, &stats, "definitely.not.a.method", json!({}));
    assert_eq!(ecode(&resp), -32601);
    assert!(emsg(&resp).contains("definitely.not.a.method"));
}

#[test]
fn status_answers_ok() {
    let (_guard, _root) = testenv::lock();
    testenv::write_config(json!({}));
    let (state, subs, stats) = harness();
    let val = rr(call(&state, &subs, &stats, "status", json!({})));
    assert_eq!(val["ok"], json!(true));
    assert_eq!(val["version"], json!(skwd_wall_core::version()));
    assert_eq!(val["service"]["name"], json!("skwd-deck"));
    assert_eq!(val["service"]["component"], json!("skwd-walld"));
    assert_eq!(val["protocol"]["name"], json!("skwd-wall"));
    assert_eq!(val["protocol"]["version"], json!(wall_proto::PROTOCOL_VERSION));
    assert_eq!(val["capabilities"], json!(wall_proto::CAPABILITIES));
    assert!(val["renderers"].is_array());
    assert!(val["library_watch"].is_object());
    assert!(val["library_watch"]["mode"].is_string());
    assert!(
        !val["renderers"].as_array().unwrap().is_empty()
            || val["renderer_error"].as_str().is_some()
    );
    let val = rr(call(&state, &subs, &stats, "paper.ready", json!({"pid": 4242})));
    assert_eq!(val["ok"], json!(true));
    let val = rr(call(&state, &subs, &stats, "subscribe", json!({})));
    assert_eq!(val["subscribed"], json!(true));
}

#[test]
fn status_includes_runtime_renderer_registry() {
    use skwd_wall_core::infrastructure::paper::{
        CapabilitiesResult, RendererCapability, RendererDiscovery, SourceKind,
    };

    let renderer = RendererCapability {
        executable: "skwd-wall-still".into(),
        source_kinds: vec![SourceKind::Static],
        video_engines: vec![],
        path: Some("/usr/bin/skwd-wall-still".into()),
        discovery: RendererDiscovery::Sibling,
        present: true,
        executable_file: true,
        dependencies: vec![],
        diagnostic: None,
    };
    let status = status_payload(Ok(CapabilitiesResult::current().with_renderers(vec![renderer])));
    assert_eq!(status["renderers"][0]["executable"], "skwd-wall-still");
    assert_eq!(status["renderers"][0]["path"], "/usr/bin/skwd-wall-still");
    assert_eq!(status["renderers"][0]["present"], true);
    assert_eq!(status["renderers"][0]["executable_file"], true);
    assert!(status.get("renderer_error").is_none());
}

#[test]
fn noctalia_preview_gated() {
    let (_guard, _root) = testenv::lock();
    testenv::write_config(json!({"theme": {"backend": "noctalia"}}));
    let (state, subs, stats) = harness();
    let resp = call(&state, &subs, &stats, "wall.shell_preview", json!({}));
    assert_eq!(ecode(&resp), -1);
    let resp = call(
        &state,
        &subs,
        &stats,
        "wall.shell_preview",
        json!({"path": "/definitely/not/a/file.png"}),
    );
    assert_eq!(ecode(&resp), -1);

    testenv::write_config(json!({"theme": {"backend": "static"}}));
    let (state, subs, stats) = harness();
    let img = std::env::temp_dir().join("skwd-noctalia-gate-test.png");
    std::fs::write(&img, b"x").unwrap();
    let resp =
        call(&state, &subs, &stats, "wall.shell_preview", json!({"path": img.to_string_lossy()}));
    assert_eq!(ecode(&resp), -1);

    testenv::write_config(json!({"theme": {"backend": "native"}}));
    let (state, subs, stats) = harness();
    let resp = rr(call(
        &state,
        &subs,
        &stats,
        "wall.shell_preview",
        json!({"path": img.to_string_lossy()}),
    ));
    assert_eq!(resp["queued"], json!(true));
    let _ = std::fs::remove_file(&img);

    let resp = rr(call(&state, &subs, &stats, "wall.shell_preview_end", json!({})));
    assert_eq!(resp["ok"], json!(true));
    assert!(state.theme().take_noctalia_preview_orig().is_none());
}

#[test]
fn rotation_wake_ok() {
    let (_guard, _root) = testenv::lock();
    testenv::write_config(json!({}));
    let (state, subs, stats) = harness();
    let val = rr(call(&state, &subs, &stats, "wall.rotation_wake", json!({})));
    assert_eq!(val["ok"], json!(true));
}

#[test]
fn apply_param_validation() {
    let (_guard, _root) = testenv::lock();
    testenv::write_config(json!({}));
    let (state, subs, stats) = harness();
    let resp = call(&state, &subs, &stats, "wall.apply", json!({"type": "we"}));
    assert_eq!(ecode(&resp), -32602);
    assert!(emsg(&resp).contains("missing we_id"));
    let resp = call(&state, &subs, &stats, "wall.apply", json!({"type": "static"}));
    assert_eq!(ecode(&resp), -32602);
    assert!(emsg(&resp).contains("missing path"));
    let resp = call(&state, &subs, &stats, "wall.apply", json!({"type": "video", "path": ""}));
    assert_eq!(ecode(&resp), -32602);
}

#[test]
fn apply_bad_type_broadcasts() {
    let (_guard, _root) = testenv::lock();
    testenv::write_config(json!({}));
    let (state, subs, stats) = harness();
    let mut rx = subscribe(&subs);
    let resp = call(
        &state,
        &subs,
        &stats,
        "wall.apply",
        json!({"type": "slideshow", "path": "/x.png", "notify": false}),
    );
    assert_eq!(ecode(&resp), -1);
    assert!(emsg(&resp).contains("not supported"));
    let evs = events(&mut rx);
    let res =
        evs.iter().find(|ev| ev.event == "skwd.wall.apply_result").expect("apply_result event");
    assert_eq!(res.data["ok"], json!(false));
    assert_eq!(res.data["error_kind"], json!("apply_failed"));
}

#[test]
fn apply_we_bad_id() {
    let (_guard, _root) = testenv::lock();
    testenv::write_config(json!({"pickOnlyMode": true}));
    let (state, subs, stats) = harness();
    let resp = call(
        &state,
        &subs,
        &stats,
        "wall.apply",
        json!({"type": "we", "we_id": "../../etc", "notify": false}),
    );
    assert_eq!(ecode(&resp), -1);
    assert!(emsg(&resp).contains("invalid WE id"));
}

#[test]
fn steam_download_gates() {
    let (_guard, _root) = testenv::lock();
    testenv::write_config(json!({}));
    let (state, subs, stats) = harness();
    let resp = call(&state, &subs, &stats, "steam.download", json!({"id": "12ab3"}));
    assert_eq!(ecode(&resp), -32602);
    let resp = call(&state, &subs, &stats, "steam.download", json!({}));
    assert_eq!(ecode(&resp), -32602);
    testenv::write_config(json!({"features": {"steam": false}}));
    let (state, subs, stats) = harness();
    let resp = call(&state, &subs, &stats, "steam.download", json!({"id": "123456789"}));
    assert_eq!(ecode(&resp), -1);
    assert!(emsg(&resp).contains("disabled"));
}

#[test]
fn source_alias_routing() {
    let (_guard, _root) = testenv::lock();
    testenv::write_config(json!({"features": {"steam": false}}));
    let (state, subs, stats) = harness();

    let direct = call(&state, &subs, &stats, "steam.download", json!({"id": "123456789"}));
    let aliased = call(
        &state,
        &subs,
        &stats,
        "source.download",
        json!({"source": "steam", "id": "123456789"}),
    );
    assert_eq!(ecode(&direct), -1);
    assert_eq!(emsg(&direct), emsg(&aliased));

    let list_alias =
        call(&state, &subs, &stats, "source.list", json!({"source": "steam", "query": "x"}));
    assert_eq!(ecode(&list_alias), -1);
    assert!(emsg(&list_alias).contains("disabled"));

    let bing = call(&state, &subs, &stats, "source.list", json!({"source": "bing"}));
    assert_eq!(ecode(&bing), -1);
    assert!(emsg(&bing).contains("bing source is disabled"));

    for bad in ["source.list", "source.download"] {
        let resp = call(&state, &subs, &stats, bad, json!({"source": "myspace"}));
        assert_eq!(ecode(&resp), -32602);
    }
    let miss = call(&state, &subs, &stats, "source.preview", json!({"source": "bing", "id": "x"}));
    assert_eq!(ecode(&miss), -32602);
}

#[test]
fn tagging_rpcs_retired() {
    let (_guard, _root) = testenv::lock();
    testenv::write_config(json!({}));
    let (state, subs, stats) = harness();
    state
        .with_db(|connection| {
            connection.execute(
                "INSERT INTO meta(key, name, type, thumb)
                 VALUES('static:review-candidate-a.png', 'review-candidate-a.png', 'static', '/thumb.webp')",
                [],
            )?;
            Ok(())
        })
        .unwrap();

    for method in [
        "analysis.start",
        "analysis.retag_one",
        "analysis.regenerate",
        "wall.review_analysis_candidate",
    ] {
        let result =
            call(&state, &subs, &stats, method, json!({"key": "static:review-candidate-a.png"}));
        assert_eq!(ecode(&result), -32601);
        assert!(emsg(&result).contains("unknown method"));
    }

    let result = rr(call(
        &state,
        &subs,
        &stats,
        "wall.update_tags",
        json!({"key": "static:review-candidate-a.png", "tags": "night,forest"}),
    ));

    assert_eq!(result["updated"], json!("static:review-candidate-a.png"));
    let row = state
        .with_db(|connection| db::list_wallpapers(connection, false))
        .unwrap()
        .into_iter()
        .find(|row| row["key"] == json!("static:review-candidate-a.png"))
        .unwrap();
    assert_eq!(row["tags"], json!("night,forest"));
}

#[test]
fn playlist_rpc_roundtrip() {
    let (_guard, _root) = testenv::lock();
    testenv::write_config(json!({}));
    let (state, subs, stats) = harness();
    let id = rr(call(&state, &subs, &stats, "playlist.create", json!({"name": "rpc test"})))["id"]
        .as_i64()
        .expect("new playlist id");
    assert!(id > 0);
    let find = |val: &Value| -> Option<Value> {
        val["playlists"].as_array().unwrap().iter().find(|item| item["id"] == json!(id)).cloned()
    };
    let listed = find(&rr(call(&state, &subs, &stats, "playlist.list", json!({}))))
        .expect("playlist listed");
    assert_eq!(listed["name"], json!("rpc test"));
    assert_eq!(listed["kind"], json!("curated"));
    assert_eq!(listed["dwell"], json!(600));

    let key = "static:pl-a.png";
    let val = rr(call(&state, &subs, &stats, "playlist.add", json!({"id": id, "key": key})));
    assert_eq!(val["in"], json!(true));
    let val = rr(call(&state, &subs, &stats, "playlist.memberships", json!({"key": key})));
    assert!(val["ids"].as_array().unwrap().contains(&json!(id)));
    let val = rr(call(&state, &subs, &stats, "playlist.toggle", json!({"id": id, "key": key})));
    assert_eq!(val["in"], json!(false));
    let val = rr(call(&state, &subs, &stats, "playlist.toggle", json!({"id": id, "key": key})));
    assert_eq!(val["in"], json!(true));
    let val = rr(call(&state, &subs, &stats, "playlist.remove", json!({"id": id, "key": key})));
    assert_eq!(val["in"], json!(false));
    let val = rr(call(&state, &subs, &stats, "playlist.memberships", json!({"key": key})));
    assert!(!val["ids"].as_array().unwrap().contains(&json!(id)));

    rr(call(
        &state,
        &subs,
        &stats,
        "playlist.update",
        json!({"id": id, "name": "renamed", "dwell": 42}),
    ));
    let listed = find(&rr(call(&state, &subs, &stats, "playlist.list", json!({})))).unwrap();
    assert_eq!(listed["name"], json!("renamed"));
    assert_eq!(listed["dwell"], json!(42));

    state
        .with_db(|connection| {
            for (key, tags) in [
                ("static:smart-match.png", "anime,sword"),
                ("static:smart-miss.png", "anime,forest"),
            ] {
                db::upsert_cache_entry(
                    connection, key, "static", key, "", "", "", "", 1, 0, 0, 0, 0, 1920, 1080,
                )?;
                connection.execute("UPDATE meta SET tags=?2 WHERE key=?1", [key, tags])?;
            }
            Ok(())
        })
        .unwrap();
    rr(call(
        &state,
        &subs,
        &stats,
        "playlist.update",
        json!({"id": id, "kind": "smart", "source": "tag:anime,sword"}),
    ));
    let listed = find(&rr(call(&state, &subs, &stats, "playlist.list", json!({})))).unwrap();
    assert_eq!(listed["count"], json!(1));
    let members = rr(call(&state, &subs, &stats, "playlist.members", json!({"id": id})));
    assert_eq!(members["members"].as_array().unwrap().len(), 1);

    rr(call(&state, &subs, &stats, "playlist.delete", json!({"id": id})));
    assert!(find(&rr(call(&state, &subs, &stats, "playlist.list", json!({})))).is_none());
}

#[test]
fn favourite_changed_flag() {
    let (_guard, _root) = testenv::lock();
    testenv::write_config(json!({}));
    let (state, subs, stats) = harness();
    state
        .with_db(|conn| {
            db::upsert_cache_entry(
                conn,
                "static:favtest.png",
                "static",
                "favtest.png",
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
    let val = rr(call(
        &state,
        &subs,
        &stats,
        "wall.set_favourite",
        json!({"key": "static:favtest.png", "favourite": true}),
    ));
    assert_eq!(val["changed"], json!(true));
    assert_eq!(val["favourite"], json!(true));
    let val = rr(call(
        &state,
        &subs,
        &stats,
        "wall.set_favourite",
        json!({"key": "static:no-such-row.png", "favourite": true}),
    ));
    assert_eq!(val["changed"], json!(false));
    state.with_db(|conn| db::delete_entries(conn, &["static:favtest.png".to_string()])).unwrap();
}

#[test]
fn remove_stays_in_library() {
    let (_guard, root) = testenv::lock();
    testenv::write_config(json!({}));
    let (state, subs, stats) = harness();
    let resp = call(&state, &subs, &stats, "wall.remove", json!({}));
    assert_eq!(ecode(&resp), 1);

    let outside = root.join("outside.png");
    std::fs::write(&outside, b"x").unwrap();
    let resp =
        call(&state, &subs, &stats, "wall.remove", json!({"path": outside.to_str().unwrap()}));
    assert!(emsg(&resp).contains("outside"));
    assert!(outside.exists());

    let inside = root.join("walls/remove-me.png");
    std::fs::write(&inside, b"x").unwrap();
    let val =
        rr(call(&state, &subs, &stats, "wall.remove", json!({"path": inside.to_str().unwrap()})));
    assert_eq!(val["removed"], json!(inside.to_str().unwrap()));
    assert!(!inside.exists());
    assert!(root.join("cache/skwd-wall-v2/deleted/remove-me.png").exists());

    let missing = root.join("walls/already-gone.png");
    state
        .with_db(|conn| {
            db::upsert_cache_entry(
                conn,
                "static:already-gone.png",
                "static",
                "already-gone.png",
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
    let mut rx = subscribe(&subs);
    let val =
        rr(call(&state, &subs, &stats, "wall.remove", json!({"path": missing.to_str().unwrap()})));
    assert_eq!(val["removed"], json!(missing.to_str().unwrap()));
    let listed = state.with_db(|conn| db::list_wallpapers(conn, false)).unwrap();
    assert!(!listed.iter().any(|item| item["key"] == "static:already-gone.png"));
    assert!(events(&mut rx).iter().any(|event| {
        event.event == "skwd.wall.removed" && event.data["key"] == "static:already-gone.png"
    }));

    let traversal = root.join("walls/../missing-outside.png");
    let response =
        call(&state, &subs, &stats, "wall.remove", json!({"path": traversal.to_str().unwrap()}));
    assert!(emsg(&response).contains("outside"));
}

#[test]
fn scan_rebroadcast() {
    let (_guard, _root) = testenv::lock();
    testenv::write_config(json!({}));
    let (state, subs, stats) = harness();
    let mut rx = subscribe(&subs);
    rr(call(&state, &subs, &stats, "scan.item", json!({"key": "static:new.png"})));
    rr(call(&state, &subs, &stats, "scan.done", json!({"count": 3, "request_id": "watch-rpc-3"})));
    let evs = events(&mut rx);
    let cached = evs.iter().find(|ev| ev.event == "skwd.wall.cached").expect("cached event");
    assert_eq!(cached.data["key"], json!("static:new.png"));
    let done = evs.iter().find(|ev| ev.event == "skwd.wall.scan_done").expect("scan_done event");
    assert_eq!(done.data["count"], json!(3));
    assert_eq!(done.data["request_id"], json!("watch-rpc-3"));
    assert!(done.data["total"].is_i64());
}

#[test]
fn optimize_start_reaches_image_worker() {
    let (_guard, _root) = testenv::lock();
    testenv::write_config(json!({}));
    testenv::reset_image_optimize_calls();
    let (state, events, stats) = harness();
    let value = rr(call(&state, &events, &stats, "optimize.start", json!({})));
    assert_eq!(value["started"], json!(true));
    assert_eq!(testenv::image_optimize_calls(), 1);
}

#[test]
fn optimize_status_query() {
    let (_guard, _root) = testenv::lock();
    testenv::write_config(json!({}));
    let (state, events, stats) = harness();
    let value = rr(call(&state, &events, &stats, "optimize.status", json!({})));
    assert_eq!(value["running"], json!(false));
}

#[test]
fn tinier_apply_queues_prep() {
    let (_guard, root) = testenv::lock();
    testenv::write_config(json!({"paper": {"videoEngine": "tinier"}}));
    let source = root.join("videos/unprepared.mp4");
    std::fs::write(&source, b"not-yet-inspected").unwrap();
    let (state, events, stats) = harness();
    let response =
        rr(call(&state, &events, &stats, "wall.apply", json!({"type": "video", "path": source})));
    assert_eq!(response["queued"], json!(true));
    assert_eq!(response["status"], json!("preparing"));
    assert!(response["task_id"].as_str().unwrap().starts_with("tinier:"));
}

#[test]
fn tinier_production_rpc_terminal_results_cover_all_outcomes() {
    use crate::testenv::TinierTestOutcome;
    let (_guard, root) = testenv::lock();
    let source = root.join("videos/terminal.mp4");
    std::fs::write(&source, b"fixture").unwrap();
    for (outcome, expected_terminal) in [
        (TinierTestOutcome::Success, Some(true)),
        (TinierTestOutcome::Failure, Some(false)),
        (TinierTestOutcome::Cancelled, None),
        (TinierTestOutcome::DelayedSuccess, None),
    ] {
        testenv::write_config(json!({"paper":{"videoEngine":"tinier"}, "pickOnlyMode":true}));
        testenv::set_tinier_outcome(outcome);
        let (state, subscribers, stats) = harness();
        let mut rx = subscribe(&subscribers);
        let response = rr(call(
            &state,
            &subscribers,
            &stats,
            "wall.apply",
            json!({"type":"video", "path":source, "output":"DP-1"}),
        ));
        assert_eq!(response["queued"], true);
        if matches!(outcome, TinierTestOutcome::DelayedSuccess) {
            state.apply().next_generation();
        }
        let terminal_expected = expected_terminal.is_some();
        let deadline = std::time::Instant::now()
            + if terminal_expected {
                std::time::Duration::from_secs(2)
            } else {
                std::time::Duration::from_millis(200)
            };
        let mut results = Vec::new();
        loop {
            results.extend(
                events(&mut rx)
                    .into_iter()
                    .filter(|event| event.event == wall_proto::ev::APPLY_RESULT),
            );
            if (terminal_expected && results.len() >= 2) || std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(results.first().unwrap().data["queued"], true);
        match expected_terminal {
            Some(ok) => {
                assert_eq!(results.len(), 2, "queued plus one terminal result");
                assert_eq!(results[1].data["ok"], ok);
            }
            None => {
                assert_eq!(results.len(), 1, "cancelled/superseded work emits no false terminal");
            }
        }
    }
    testenv::set_tinier_outcome(TinierTestOutcome::Failure);
}

#[test]
fn optimize_start_converts_image() {
    let (_guard, root) = testenv::lock();
    let runtime = crate::infrastructure::platform::build_runtime().unwrap();
    let _runtime = runtime.enter();
    let wallpaper_dir = tempfile::tempdir_in(root).unwrap();
    let source = wallpaper_dir.path().join("rpc-fixture.png");
    let pixels = image::RgbaImage::from_fn(640, 360, |x, y| {
        image::Rgba([(x % 255) as u8, (y % 255) as u8, ((x + y) % 255) as u8, 255])
    });
    pixels.save(&source).unwrap();
    testenv::write_config(json!({
        "paths": { "wallpaper": wallpaper_dir.path().to_string_lossy() },
        "performance": {
            "imageOptimizePreset": "light",
            "imageOptimizeResolution": "1080p"
        }
    }));
    let (state, events, stats) = harness();
    let mut ctx = testenv::context(&state, &events, &stats);
    ctx.workers = std::sync::Arc::new(crate::infrastructure::processes::ProcessSupervisor::new(
        std::sync::Arc::clone(&state),
        state.config_store(),
        state.database(),
        std::sync::Arc::clone(&events)
            as std::sync::Arc<dyn crate::backend::events::EventPublisher>,
        std::sync::Arc::clone(&ctx.tasks),
        std::sync::Arc::clone(&stats),
        false,
    ));
    let response = super::dispatch(
        &ctx,
        &wall_proto::Request { method: "optimize.start".into(), params: json!({}), id: 99 },
    );
    assert_eq!(rr(response)["started"], json!(true));

    let destination = wallpaper_dir.path().join("rpc-fixture.webp");
    for _ in 0..100 {
        if destination.is_file() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let status = rr(super::dispatch(
        &ctx,
        &wall_proto::Request { method: "optimize.status".into(), params: json!({}), id: 100 },
    ));
    assert!(destination.is_file(), "optimizer status: {status}");
    assert_eq!(status["optimized"], json!(1));
    assert!(status["future_saved_bytes"].as_u64().unwrap_or(0) > 0);
    assert!(status["temporary_overhead_bytes"].as_u64().unwrap_or(0) > 0);
    assert!(!source.exists());
    assert!(std::fs::metadata(&destination).unwrap().len() > 0);
    assert!(wallpaper_dir.path().join(".skwd-wall-v2/trash/images/rpc-fixture.png").is_file());
}

#[test]
fn handle_conn_lifecycle() {
    use std::io::{BufRead, BufReader, Write};
    let (_guard, root) = testenv::lock();
    testenv::write_config(json!({}));
    let (state, subs, stats) = harness();
    let pause_output = root.join("picker-pause.stdin");
    let mut child = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("exec cat > '{}'", pause_output.display()))
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let stdin = child.stdin.take();
    state.renderers().set_video_paper("DP-1", child, stdin);
    let (client, server) = std::os::unix::net::UnixStream::pair().unwrap();
    client.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
    let ctx = testenv::context(&state, &subs, &stats);
    let runtime = testenv::runtime();
    let task = runtime.spawn(async move {
        server.set_nonblocking(true).unwrap();
        let server = tokio::net::UnixStream::from_std(server).unwrap();
        super::handle_conn(server, &ctx).await;
    });
    let mut writer = client.try_clone().unwrap();
    let mut reader = BufReader::new(client);
    let mut line = String::new();

    writer.write_all(b"this is not json\n").unwrap();
    reader.read_line(&mut line).unwrap();
    let resp: Response = serde_json::from_str(&line).unwrap();
    assert_eq!(ecode(&resp), -32700);

    line.clear();
    writer.write_all(b"{\"method\":\"status\",\"id\":3}\n").unwrap();
    reader.read_line(&mut line).unwrap();
    let resp: Response = serde_json::from_str(&line).unwrap();
    assert_eq!(resp.id, 3);
    assert_eq!(rr(resp)["version"], json!(skwd_wall_core::version()));

    line.clear();
    writer.write_all(b"{\"method\":\"picker.session.begin\",\"id\":7}\n").unwrap();
    reader.read_line(&mut line).unwrap();
    let resp: Response = serde_json::from_str(&line).unwrap();
    assert_eq!(resp.id, 7);
    assert_eq!(rr(resp)["ok"], json!(true));

    line.clear();
    writer.write_all(b"{\"method\":\"subscribe\",\"id\":4}\n").unwrap();
    reader.read_line(&mut line).unwrap();
    line.clear();
    writer.write_all(b"{\"method\":\"subscribe\",\"id\":5}\n").unwrap();
    reader.read_line(&mut line).unwrap();
    assert_eq!(subs.subscriber_count(), 1);

    line.clear();
    writer.write_all(b"{\"method\":\"wall.list\",\"id\":6}\n").unwrap();
    reader.read_line(&mut line).unwrap();
    let val: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(val["id"], json!(6));
    assert!(val["result"]["count"].is_i64());

    drop(writer);
    drop(reader);
    runtime.block_on(task).unwrap();
    assert_eq!(subs.subscriber_count(), 0);
    for (mut child, stdin) in state.renderers().take_all_video_papers() {
        drop(stdin);
        let _ = child.wait();
    }
    assert_eq!(std::fs::read_to_string(pause_output).unwrap(), "");
}

#[test]
fn ready_fast_path_saturated_pool() {
    use std::io::{BufRead, BufReader, Write};
    use std::sync::atomic::{AtomicBool, Ordering};
    let (_guard, _root) = testenv::lock();
    testenv::write_config(json!({}));
    let (state, subs, stats) = harness();
    let ctx = testenv::context(&state, &subs, &stats);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(4)
        .enable_all()
        .build()
        .unwrap();
    let stall = std::sync::Arc::new(AtomicBool::new(true));
    for _ in 0..64 {
        let stall = std::sync::Arc::clone(&stall);
        runtime.spawn(async move {
            let _ = tokio::task::spawn_blocking(move || {
                while stall.load(Ordering::Acquire) {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            })
            .await;
        });
    }
    let (client, server) = std::os::unix::net::UnixStream::pair().unwrap();
    client.set_read_timeout(Some(std::time::Duration::from_millis(200))).unwrap();
    let task = runtime.spawn(async move {
        server.set_nonblocking(true).unwrap();
        let server = tokio::net::UnixStream::from_std(server).unwrap();
        super::handle_conn(server, &ctx).await;
    });
    let mut writer = client.try_clone().unwrap();
    let mut reader = BufReader::new(client);
    let mut line = String::new();
    let started = std::time::Instant::now();
    writer
        .write_all(b"{\"method\":\"paper.ready\",\"id\":11,\"params\":{\"pid\":424242}}\n")
        .unwrap();
    reader.read_line(&mut line).expect("paper.ready reply");
    assert!(
        started.elapsed() < std::time::Duration::from_millis(200),
        "elapsed {:?}",
        started.elapsed()
    );
    let resp: Response = serde_json::from_str(&line).unwrap();
    assert_eq!(rr(resp)["ok"], json!(true));
    stall.store(false, Ordering::Release);
    drop(writer);
    drop(reader);
    runtime.block_on(task).unwrap();
}

#[test]
fn disconnect_restores_scheme() {
    use std::io::{BufRead, BufReader, Write};
    let (_guard, _root) = testenv::lock();
    testenv::write_config(json!({}));
    let (state, subs, stats) = harness();
    let ctx = testenv::context(&state, &subs, &stats);
    state.theme().arm_bridge_preview(b"{}".to_vec());
    let generation = state.theme().shell_preview_generation();
    let (client, server) = std::os::unix::net::UnixStream::pair().unwrap();
    client.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
    let runtime = testenv::runtime();
    let task = runtime.spawn(async move {
        server.set_nonblocking(true).unwrap();
        let server = tokio::net::UnixStream::from_std(server).unwrap();
        super::handle_conn(server, &ctx).await;
    });
    let mut writer = client.try_clone().unwrap();
    let mut reader = BufReader::new(client);
    let mut line = String::new();
    writer.write_all(b"{\"method\":\"wall.shell_preview\",\"id\":8,\"params\":{}}\n").unwrap();
    reader.read_line(&mut line).unwrap();
    drop(writer);
    drop(reader);
    runtime.block_on(task).unwrap();
    assert!(state.theme().shell_preview_generation() > generation);
    assert!(!state.theme().bridge_preview_armed());
}

#[test]
fn retheme_handoff() {
    let (_guard, _root) = testenv::lock();
    let (state, subs, stats) = harness();
    let resp = call(&state, &subs, &stats, "wall.retheme", json!({}));
    assert_eq!(ecode(&resp), 1);
    state.theme().set_source("/tmp/skwd-retheme-test.png");
    let val = rr(call(&state, &subs, &stats, "wall.retheme", json!({})));
    assert_eq!(val["rethemed"], json!(true));
    let val = rr(call(&state, &subs, &stats, "wall.retheme", json!({"scheme": "scheme-rainbow"})));
    assert_eq!(val["rethemed"], json!(true));
}

#[test]
fn theme_preview_profiles() {
    let (_guard, _root) = testenv::lock();
    testenv::write_config(json!({
        "theme": {
            "policy": "fixed",
            "staticTheme": "nord",
            "style": "natural",
            "mode": "dark"
        }
    }));
    let (state, subs, stats) = harness();
    state.theme().set_source("/unused-for-static-theme.png");
    let value = rr(call(&state, &subs, &stats, "theme.previews", json!({})));
    let previews = value["previews"].as_array().expect("preview list");
    assert_eq!(value["backend"], json!("static"));
    assert!(value["backends"].as_array().is_some_and(|backends| !backends.is_empty()));
    assert_eq!(previews[0]["key"], json!(skwd_config::keys::theme::STATIC_THEME));
    assert_eq!(previews[0]["value"], json!("nord"));
    assert_eq!(previews[0]["label"], json!("Nord"));
    assert!(previews.iter().all(|preview| preview["palette"]["surfaceText"].is_string()));
}

#[test]
fn theme_backends_detected() {
    let (_guard, _root) = testenv::lock();
    let (state, subs, stats) = harness();
    let val = rr(call(&state, &subs, &stats, "theme.backends", json!({})));
    let names: Vec<&str> =
        val["backends"].as_array().unwrap().iter().filter_map(|entry| entry.as_str()).collect();
    assert!(names.contains(&"off") && names.contains(&"native"));
    assert!(names.iter().all(|name| wall_proto::THEME_BACKENDS.contains(name)), "{names:?}");
    for built_in in ["off", "native", "static", "skwd-iris"] {
        assert!(names.contains(&built_in), "{built_in}");
    }
}

#[test]
fn remove_we_trashes_scene() {
    let (_guard, root) = testenv::lock();
    testenv::write_config(
        json!({ "paths": { "steamWorkshop": root.join("we").to_string_lossy() } }),
    );
    let (state, subs, stats) = harness();
    let mut rx = subscribe(&subs);
    let scene = root.join("we").join("4242");
    std::fs::create_dir_all(&scene).unwrap();
    std::fs::write(scene.join("scene.pkg"), b"x").unwrap();

    let resp = call(&state, &subs, &stats, "wall.remove", json!({ "we_id": "../4242" }));
    assert_eq!(ecode(&resp), 1);
    let resp = call(&state, &subs, &stats, "wall.remove", json!({ "we_id": "99999" }));
    assert_eq!(ecode(&resp), 1);

    let val = rr(call(&state, &subs, &stats, "wall.remove", json!({ "we_id": "4242" })));
    assert_eq!(val["removed"], "we:4242");
    assert!(!scene.exists());
    assert!(root.join("cache/skwd-wall-v2/deleted/4242/scene.pkg").is_file());
    assert!(events(&mut rx).iter().any(|ev| ev.event == "skwd.wall.removed"));
}

#[test]
fn remove_we_symlink_keeps_target() {
    let (_guard, root) = testenv::lock();
    let _ = std::fs::remove_dir_all(root.join("cache/skwd-wall-v2/deleted/5555.content"));
    let _ = std::fs::remove_dir_all(root.join("steamcmd-root/5555"));
    testenv::write_config(
        json!({ "paths": { "steamWorkshop": root.join("we").to_string_lossy() } }),
    );
    let (state, subs, stats) = harness();
    let content = root.join("steamcmd-root/5555");
    std::fs::create_dir_all(&content).unwrap();
    std::fs::write(content.join("scene.pkg"), b"x").unwrap();
    std::fs::create_dir_all(root.join("we")).unwrap();
    let link = root.join("we/5555");
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(&content, &link).unwrap();

    let val = rr(call(&state, &subs, &stats, "wall.remove", json!({ "we_id": "5555" })));
    assert_eq!(val["removed"], "we:5555");
    assert!(!link.exists());
    assert!(content.join("scene.pkg").is_file());
    assert!(!root.join("cache/skwd-wall-v2/deleted/5555.content").exists());
}

#[test]
fn remove_we_symlink_steam_content() {
    let (_guard, root) = testenv::lock();
    let _ = std::fs::remove_dir_all(root.join("cache/skwd-wall-v2/deleted/5555.content"));
    let steam = root.join("steamcmd-root");
    let content = steam.join("steamapps/workshop/content/431960/5555");
    testenv::write_config(json!({
        "paths": {
            "steamWorkshop": root.join("we").to_string_lossy(),
            "steam": steam.to_string_lossy()
        }
    }));
    let (state, subs, stats) = harness();
    std::fs::create_dir_all(&content).unwrap();
    std::fs::write(content.join("scene.pkg"), b"x").unwrap();
    std::fs::create_dir_all(root.join("we")).unwrap();
    let link = root.join("we/5555");
    std::os::unix::fs::symlink(&content, &link).unwrap();

    let val = rr(call(&state, &subs, &stats, "wall.remove", json!({ "we_id": "5555" })));
    assert_eq!(val["removed"], "we:5555");
    assert!(!link.exists());
    assert!(!content.exists());
    assert!(root.join("cache/skwd-wall-v2/deleted/5555.content/scene.pkg").is_file());
}

#[test]
fn trash_no_clobber() {
    let (_guard, root) = testenv::lock();
    let trash = root.join("trash-uniq");
    std::fs::create_dir_all(&trash).unwrap();
    let art = root.join("art");
    let minimal = root.join("minimal");
    std::fs::create_dir_all(&art).unwrap();
    std::fs::create_dir_all(&minimal).unwrap();
    std::fs::write(art.join("red.jpg"), b"art-red").unwrap();
    std::fs::write(minimal.join("red.jpg"), b"minimal-red").unwrap();
    crate::infrastructure::removal::trash_file(&art.join("red.jpg"), &trash).unwrap();
    crate::infrastructure::removal::trash_file(&minimal.join("red.jpg"), &trash).unwrap();
    assert_eq!(std::fs::read(trash.join("red.jpg")).unwrap(), b"art-red");
    assert_eq!(std::fs::read(trash.join("red.jpg.1")).unwrap(), b"minimal-red");
}

#[test]
fn trash_dir_unique() {
    let (_guard, root) = testenv::lock();
    let trash = root.join("trash-dir");
    std::fs::create_dir_all(&trash).unwrap();
    let scene = root.join("scene-abc");
    std::fs::create_dir_all(scene.join("nested")).unwrap();
    std::fs::write(scene.join("nested/file.pkg"), b"payload").unwrap();
    let existing = trash.join("abc");
    std::fs::create_dir_all(&existing).unwrap();
    std::fs::write(existing.join("prior"), b"x").unwrap();
    crate::infrastructure::removal::trash_dir(&scene, &trash, "abc").unwrap();
    assert!(!scene.exists());
    assert_eq!(std::fs::read(trash.join("abc.1/nested/file.pkg")).unwrap(), b"payload");
}

#[test]
fn remove_reconciles_video() {
    let (_guard, root) = testenv::lock();
    testenv::write_config(json!({
        "paths": { "wallpaper": root.join("walls").to_string_lossy(),
                   "videoWallpaper": root.join("videos").to_string_lossy() }
    }));
    let (state, subs, stats) = harness();
    let vid = root.join("videos/clip.mp4");
    std::fs::write(&vid, b"v").unwrap();
    let vid_path = vid.to_string_lossy().to_string();
    let tinier = root.join("cache/skwd-wall-v2/video-opt/clip.tinier-v1.ivf");
    std::fs::create_dir_all(tinier.parent().unwrap()).unwrap();
    std::fs::write(&tinier, b"av1").unwrap();
    let key = format!("video:{}", "clip.mp4");
    state
        .with_db(|conn| {
            db::tinier_convert_record(
                conn,
                &vid_path,
                &tinier.to_string_lossy(),
                "30/1",
                db::TINIER_CONVERT_PRESET,
                1,
                3,
            )?;
            let pid = db::playlist_create(conn, "mine")?;
            db::playlist_add_member(conn, pid, &key)?;
            Ok(pid)
        })
        .unwrap();
    std::fs::write(
        root.join("cache/skwd-wall-v2/last-wallpaper.json"),
        json!({"type": "video", "path": vid_path, "we_id": ""}).to_string(),
    )
    .unwrap();

    let val = rr(call(&state, &subs, &stats, "wall.remove", json!({ "path": vid_path })));
    assert_eq!(val["removed"], vid_path);
    assert!(!tinier.exists());
    let members = state.with_db(|conn| db::playlist_memberships_for_key(conn, &key)).unwrap();
    assert!(members.is_empty());
    let dest = state.with_db(|conn| db::tinier_convert_entry(conn, &vid_path)).unwrap();
    assert!(dest.is_none());
    assert!(!root.join("cache/skwd-wall-v2/last-wallpaper.json").exists());
}
