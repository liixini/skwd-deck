use super::{SupersededApply, apply_core, apply_locked_fanout, key_apply_args, theme_delay};
use crate::backend::history::ApplySource;
use crate::testenv;
use skwd_wall_core::backend::wallpaper::{
    ApplyOutputRequest, ApplyStaticRequest, ApplyVideoRequest, StaticSmartRequest,
    VideoTransitionRequest, WallpaperApplication,
};

struct FailingApplication;

impl WallpaperApplication for FailingApplication {
    fn apply_static(&self, _: ApplyStaticRequest<'_>) -> anyhow::Result<()> {
        anyhow::bail!("injected static failure")
    }

    fn apply_static_smart(&self, _: StaticSmartRequest<'_>) -> anyhow::Result<()> {
        anyhow::bail!("injected smart failure")
    }

    fn apply_video(&self, _: ApplyVideoRequest<'_>) -> anyhow::Result<()> {
        anyhow::bail!("injected video failure")
    }

    fn apply_output(&self, _: ApplyOutputRequest<'_>) -> anyhow::Result<()> {
        anyhow::bail!("injected output failure")
    }

    fn apply_video_transition(&self, _: VideoTransitionRequest<'_>) -> anyhow::Result<()> {
        anyhow::bail!("injected transition failure")
    }

    fn apply_we(&self, _: &str) -> anyhow::Result<Option<String>> {
        anyhow::bail!("injected WE failure")
    }

    fn video_engine_is_vk(&self) -> bool {
        true
    }

    fn reload_we(&self) -> anyhow::Result<()> {
        anyhow::bail!("injected reload failure")
    }
}

struct SupersedingApplication(std::sync::Arc<skwd_wall_core::WallState>);

impl SupersedingApplication {
    fn supersede(&self) {
        self.0.apply().next_generation();
    }
}

struct FlakyApplication(std::sync::atomic::AtomicBool);

impl FlakyApplication {
    fn new() -> Self {
        Self(std::sync::atomic::AtomicBool::new(false))
    }

    fn attempt(&self) -> anyhow::Result<()> {
        if self.0.swap(true, std::sync::atomic::Ordering::SeqCst) {
            Ok(())
        } else {
            anyhow::bail!("retryable injected failure")
        }
    }
}

impl WallpaperApplication for FlakyApplication {
    fn apply_static(&self, _: ApplyStaticRequest<'_>) -> anyhow::Result<()> {
        self.attempt()
    }
    fn apply_static_smart(&self, _: StaticSmartRequest<'_>) -> anyhow::Result<()> {
        self.attempt()
    }
    fn apply_video(&self, _: ApplyVideoRequest<'_>) -> anyhow::Result<()> {
        self.attempt()
    }
    fn apply_output(&self, _: ApplyOutputRequest<'_>) -> anyhow::Result<()> {
        self.attempt()
    }
    fn apply_video_transition(&self, _: VideoTransitionRequest<'_>) -> anyhow::Result<()> {
        self.attempt()
    }
    fn apply_we(&self, _: &str) -> anyhow::Result<Option<String>> {
        self.attempt()?;
        Ok(None)
    }
    fn video_engine_is_vk(&self) -> bool {
        true
    }
    fn reload_we(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

impl WallpaperApplication for SupersedingApplication {
    fn apply_static(&self, _: ApplyStaticRequest<'_>) -> anyhow::Result<()> {
        self.supersede();
        Ok(())
    }

    fn apply_static_smart(&self, _: StaticSmartRequest<'_>) -> anyhow::Result<()> {
        self.supersede();
        Ok(())
    }

    fn apply_video(&self, _: ApplyVideoRequest<'_>) -> anyhow::Result<()> {
        self.supersede();
        Ok(())
    }

    fn apply_output(&self, _: ApplyOutputRequest<'_>) -> anyhow::Result<()> {
        self.supersede();
        Ok(())
    }

    fn apply_video_transition(&self, _: VideoTransitionRequest<'_>) -> anyhow::Result<()> {
        self.supersede();
        Ok(())
    }

    fn apply_we(&self, _: &str) -> anyhow::Result<Option<String>> {
        self.supersede();
        Ok(None)
    }

    fn video_engine_is_vk(&self) -> bool {
        true
    }

    fn reload_we(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

#[test]
fn theme_delay_zero() {
    assert_eq!(theme_delay(), std::time::Duration::ZERO);
}

#[test]
fn key_apply_shapes() {
    assert_eq!(
        key_apply_args("static:sub/a.jpg", "/w", "/v"),
        ("static", "/w/sub/a.jpg".to_string(), String::new())
    );
    assert_eq!(
        key_apply_args("video:b.mp4", "/w", "/v"),
        ("video", "/v/b.mp4".to_string(), String::new())
    );
    assert_eq!(
        key_apply_args("static:a.png", "/w/", "/v/"),
        ("static", "/w/a.png".to_string(), String::new()),
    );
    assert_eq!(key_apply_args("we:12345", "/w", "/v"), ("we", String::new(), "12345".to_string()));
    assert_eq!(
        key_apply_args("/abs/pic.png", "/w", "/v"),
        ("static", "/abs/pic.png".to_string(), String::new()),
    );
}

#[test]
fn failed_execution_cannot_publish_for_any_media_or_output_scope() {
    let (_guard, root) = testenv::lock();
    let we_root = root.join("we");
    let we_item = we_root.join("123");
    std::fs::create_dir_all(&we_item).unwrap();
    std::fs::write(we_item.join("project.json"), r#"{"type":"scene","file":"scene.json"}"#)
        .unwrap();
    testenv::write_config(serde_json::json!({
        "paths": { "steamWorkshop": we_root.to_string_lossy() },
        "features": { "steam": true },
        "pickOnlyMode": false,
        "history": { "enabled": true }
    }));
    let (state, publisher, stats) = testenv::harness();
    let history =
        crate::infrastructure::history::FileHistoryRepository::new(state.config().cache_dir());
    state.theme().set_source("sentinel-theme");
    let mut events = testenv::subscribe(&publisher);
    let cache = root.join("cache/skwd-wall-v2");
    let _ = std::fs::remove_file(cache.join("last-applied.json"));

    for (kind, path, we_id) in [
        (wall_proto::kind::STATIC, "/wall/fail.png", ""),
        (wall_proto::kind::VIDEO, "/wall/fail.mp4", ""),
        (wall_proto::kind::WE, "", "123"),
    ] {
        for output in ["*", "DP-1"] {
            let _ = std::fs::remove_file(cache.join("last-wallpaper.json"));
            let _ = std::fs::remove_file(cache.join("history.json"));
            let result = apply_core(
                &state,
                &FailingApplication,
                &history,
                publisher.as_ref(),
                &stats,
                kind,
                path,
                we_id,
                true,
                0,
                ApplySource::User,
                output,
                false,
                false,
                None,
                None,
            );
            assert!(result.is_err(), "{kind} {output} unexpectedly succeeded");
            assert!(!cache.join("last-wallpaper.json").exists());
            assert!(!cache.join("history.json").exists());
            assert_eq!(state.theme().source().as_deref(), Some("sentinel-theme"));
        }
    }
    assert!(
        testenv::events(&mut events).iter().all(|event| event.event != wall_proto::ev::APPLIED)
    );
    assert_eq!(stats.counters_json()["applies"], 0);
    assert!(!cache.join("last-applied.json").exists());
}

#[test]
fn superseded_handoff_cannot_publish_for_any_media_or_output_scope() {
    let (_guard, root) = testenv::lock();
    let we_root = root.join("we-superseded");
    let we_item = we_root.join("456");
    std::fs::create_dir_all(&we_item).unwrap();
    std::fs::write(we_item.join("project.json"), r#"{"type":"scene","file":"scene.json"}"#)
        .unwrap();
    testenv::write_config(serde_json::json!({
        "paths": { "steamWorkshop": we_root.to_string_lossy() },
        "features": { "steam": true },
        "pickOnlyMode": false,
        "history": { "enabled": true }
    }));
    let (state, publisher, stats) = testenv::harness();
    let history =
        crate::infrastructure::history::FileHistoryRepository::new(state.config().cache_dir());
    let application = SupersedingApplication(std::sync::Arc::clone(&state));
    state.theme().set_source("superseded-sentinel");
    let mut events = testenv::subscribe(&publisher);
    let cache = root.join("cache/skwd-wall-v2");
    let _ = std::fs::remove_file(cache.join("last-applied.json"));

    for (kind, path, we_id) in [
        (wall_proto::kind::STATIC, "/wall/late.png", ""),
        (wall_proto::kind::VIDEO, "/wall/late.mp4", ""),
        (wall_proto::kind::WE, "", "456"),
    ] {
        for output in ["*", "DP-1"] {
            let _ = std::fs::remove_file(cache.join("last-wallpaper.json"));
            let _ = std::fs::remove_file(cache.join("history.json"));
            let result = apply_core(
                &state,
                &application,
                &history,
                publisher.as_ref(),
                &stats,
                kind,
                path,
                we_id,
                true,
                0,
                ApplySource::User,
                output,
                false,
                false,
                None,
                None,
            );
            assert!(
                result.is_err_and(|error| error.to_string().contains("superseded apply")),
                "{kind} {output} published after supersession"
            );
            assert!(!cache.join("last-wallpaper.json").exists());
            assert!(!cache.join("history.json").exists());
            assert_eq!(state.theme().source().as_deref(), Some("superseded-sentinel"));
        }
    }
    assert!(
        testenv::events(&mut events).iter().all(|event| event.event != wall_proto::ev::APPLIED)
    );
    assert_eq!(stats.counters_json()["applies"], 0);
    assert!(!cache.join("last-applied.json").exists());
}

#[test]
fn retry_publishes_once_for_each_media_and_output_scope() {
    let (_guard, root) = testenv::lock();
    let we_root = root.join("we-retry");
    let we_item = we_root.join("789");
    std::fs::create_dir_all(&we_item).unwrap();
    std::fs::write(we_item.join("project.json"), r#"{"type":"scene","file":"scene.json"}"#)
        .unwrap();
    testenv::write_config(serde_json::json!({
        "paths": { "steamWorkshop": we_root.to_string_lossy() },
        "features": { "steam": true },
        "pickOnlyMode": false,
        "history": { "enabled": true }
    }));
    let (state, publisher, stats) = testenv::harness();
    let history =
        crate::infrastructure::history::FileHistoryRepository::new(state.config().cache_dir());
    let mut events = testenv::subscribe(&publisher);

    for (kind, path, we_id) in [
        (wall_proto::kind::STATIC, "/wall/retry.png", ""),
        (wall_proto::kind::VIDEO, "/wall/retry.mp4", ""),
        (wall_proto::kind::WE, "", "789"),
    ] {
        for output in ["*", "DP-1"] {
            let application = FlakyApplication::new();
            let invoke = || {
                apply_core(
                    &state,
                    &application,
                    &history,
                    publisher.as_ref(),
                    &stats,
                    kind,
                    path,
                    we_id,
                    true,
                    0,
                    ApplySource::User,
                    output,
                    false,
                    false,
                    None,
                    None,
                )
            };
            assert!(invoke().is_err(), "{kind} {output} first attempt must fail");
            assert!(invoke().is_ok(), "{kind} {output} retry must succeed");
        }
    }
    let applied = testenv::events(&mut events)
        .into_iter()
        .filter(|event| event.event == wall_proto::ev::APPLIED)
        .count();
    assert_eq!(applied, 6, "each successful retry publishes exactly once");
    assert_eq!(stats.counters_json()["applies"], 6);
}

#[test]
fn supersession_is_typed_and_same_text_renderer_failure_is_not() {
    let typed = anyhow::Error::new(SupersededApply);
    let renderer = anyhow::anyhow!("superseded apply");
    assert!(typed.downcast_ref::<SupersededApply>().is_some());
    assert!(renderer.downcast_ref::<SupersededApply>().is_none());
}

struct OneShotSupersedingApplication {
    state: std::sync::Arc<skwd_wall_core::WallState>,
    cache: String,
    fired: std::sync::atomic::AtomicBool,
}

impl WallpaperApplication for OneShotSupersedingApplication {
    fn apply_static(&self, _: ApplyStaticRequest<'_>) -> anyhow::Result<()> {
        unreachable!()
    }
    fn apply_static_smart(&self, _: StaticSmartRequest<'_>) -> anyhow::Result<()> {
        unreachable!()
    }
    fn apply_video(&self, _: ApplyVideoRequest<'_>) -> anyhow::Result<()> {
        unreachable!()
    }
    fn apply_video_transition(&self, _: VideoTransitionRequest<'_>) -> anyhow::Result<()> {
        unreachable!()
    }
    fn apply_we(&self, _: &str) -> anyhow::Result<Option<String>> {
        unreachable!()
    }
    fn video_engine_is_vk(&self) -> bool {
        true
    }
    fn reload_we(&self) -> anyhow::Result<()> {
        Ok(())
    }
    fn apply_output(&self, _: ApplyOutputRequest<'_>) -> anyhow::Result<()> {
        let desired = skwd_wall_core::audio::read_state(&self.cache);
        let assignments = desired
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(output, entry)| {
                let path = entry.get("path")?.as_str()?.to_string();
                Some((output.clone(), path))
            })
            .collect();
        self.state.renderers().replace_assignments(assignments);
        if !self.fired.swap(true, std::sync::atomic::Ordering::SeqCst) {
            self.state.apply().next_generation();
        }
        Ok(())
    }
}

#[test]
fn newer_generation_deterministically_beats_the_entire_locked_fanout() {
    let (_guard, _root) = testenv::lock();
    testenv::write_config(serde_json::json!({}));
    let (state, publisher, stats) = testenv::harness();
    let history =
        crate::infrastructure::history::FileHistoryRepository::new(state.config().cache_dir());
    let baseline = serde_json::json!({
        "*": skwd_wall_core::audio::entry("video", "/old.mp4", "", true, 0),
    });
    skwd_wall_core::audio::write_state(&state.config().cache_dir(), &baseline);
    state.renderers().set_assignment("*", "/old.mp4");
    let app = OneShotSupersedingApplication {
        state: std::sync::Arc::clone(&state),
        cache: state.config().cache_dir(),
        fired: std::sync::atomic::AtomicBool::new(false),
    };
    let before = state.apply().generation();
    let error = apply_locked_fanout(
        &state,
        &app,
        &history,
        publisher.as_ref(),
        &stats,
        "video",
        "/new.mp4",
        "",
        true,
        0,
        ApplySource::User,
        &["DP-1".into(), "DP-2".into()],
        &["LOCKED".into()],
        false,
        false,
        None,
        None,
    )
    .unwrap_err();
    assert!(error.downcast_ref::<SupersededApply>().is_some());
    assert_eq!(state.apply().generation(), before + 2, "one fanout claim plus one newer request");
    assert_eq!(skwd_wall_core::audio::read_state(&state.config().cache_dir()), baseline);
    assert_eq!(
        state.renderers().assignments(),
        std::collections::HashMap::from([("*".to_string(), "/old.mp4".to_string())])
    );
}

#[test]
fn successful_locked_fanout_publishes_each_committed_output_in_production_path() {
    let (_guard, root) = testenv::lock();
    testenv::write_config(serde_json::json!({
        "pickOnlyMode": false,
        "history": {"enabled": true}
    }));
    let wall = root.join("walls/fanout.png");
    std::fs::write(&wall, b"png").unwrap();
    let path = wall.to_string_lossy().into_owned();
    let (state, publisher, stats) = testenv::harness();
    state
        .with_db(|db| {
            skwd_wall_core::db::upsert_cache_entry(
                db,
                "static:fanout.png",
                "static",
                "fanout.png",
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
    let history =
        crate::infrastructure::history::FileHistoryRepository::new(state.config().cache_dir());
    let cache = state.config().cache_dir();
    for name in ["last-wallpaper.json", "history.json", "last-applied.json", "outputs.json"] {
        let _ = std::fs::remove_file(std::path::Path::new(&cache).join(name));
    }
    skwd_wall_core::audio::set_entry(&cache, "LOCKED", "static", "/keep.png", "", true, 0);
    let mut events = testenv::subscribe(&publisher);
    let outputs = vec!["DP-1".to_string(), "DP-2".to_string(), "DP-3".to_string()];
    let application = FlakyApplication(std::sync::atomic::AtomicBool::new(true));

    let result = apply_locked_fanout(
        &state,
        &application,
        &history,
        publisher.as_ref(),
        &stats,
        "static",
        &path,
        "",
        true,
        0,
        ApplySource::User,
        &outputs,
        &["LOCKED".into()],
        false,
        false,
        None,
        None,
    )
    .unwrap();

    assert_eq!(result["applied"], serde_json::json!(outputs));
    let desired = skwd_wall_core::audio::read_state(&cache);
    for output in &outputs {
        assert_eq!(desired[output]["path"], path);
    }
    assert_eq!(desired["LOCKED"]["path"], "/keep.png");
    let applied: Vec<_> = testenv::events(&mut events)
        .into_iter()
        .filter(|event| event.event == wall_proto::ev::APPLIED)
        .collect();
    assert_eq!(applied.len(), 3);
    let mut event_outputs: Vec<_> =
        applied.iter().filter_map(|event| event.data["output"].as_str()).collect();
    event_outputs.sort_unstable();
    assert_eq!(event_outputs, ["DP-1", "DP-2", "DP-3"]);
    assert_eq!(stats.counters_json()["applies"], 3);
    let last: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::path::Path::new(&cache).join("last-wallpaper.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(last["path"], path);
    let restored = crate::infrastructure::restore_policy::read_last_applied();
    assert_eq!(restored.any.as_ref().map(|wallpaper| wallpaper.path.as_str()), Some(path.as_str()));
    let rows = state.with_db(|db| skwd_wall_core::db::list_wallpapers(db, false)).unwrap();
    assert_eq!(
        rows.iter().find(|row| row["key"] == "static:fanout.png").unwrap()["apply_count"],
        3
    );
    let history_json: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::path::Path::new(&cache).join("history.json")).unwrap(),
    )
    .unwrap();
    for output in &outputs {
        assert_eq!(history_json[output]["entries"].as_array().unwrap().len(), 1);
    }
}
