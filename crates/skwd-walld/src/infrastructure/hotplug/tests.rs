use super::{RESTORE_RETRY_DELAYS, SupersededApply, retry_restore};
use std::time::Duration;

#[test]
fn retries_transient_failure() {
    let mut calls = 0;
    let _ = retry_restore(&[Duration::ZERO; 4], || {
        calls += 1;
        if calls < 3 {
            anyhow::bail!("backend unavailable");
        }
        Ok(())
    });
    assert_eq!(calls, 3);
}

#[test]
fn bounds_failed_restore() {
    let mut calls = 0;
    let _ = retry_restore(&[Duration::ZERO; RESTORE_RETRY_DELAYS.len()], || {
        calls += 1;
        anyhow::bail!("backend unavailable")
    });
    assert_eq!(calls, RESTORE_RETRY_DELAYS.len() + 1);
}

#[test]
fn stops_after_success() {
    let mut calls = 0;
    let _ = retry_restore(&RESTORE_RETRY_DELAYS, || {
        calls += 1;
        Ok(())
    });
    assert_eq!(calls, 1);
}

#[test]
fn stops_when_superseded() {
    let mut calls = 0;
    let _ = retry_restore(&RESTORE_RETRY_DELAYS, || {
        calls += 1;
        Err(SupersededApply.into())
    });
    assert_eq!(calls, 1);
}

use crate::backend::history::ApplySource;
use crate::testenv;
use skwd_wall_core::backend::wallpaper::{
    ApplyOutputRequest, ApplyStaticRequest, ApplyVideoRequest, OutputTransitionRequest,
    StaticSmartRequest, VideoTransitionRequest, WallpaperApplication,
};
use std::sync::{Arc, Mutex};

struct Backend {
    state: Arc<skwd_wall_core::WallState>,
    calls: Mutex<Vec<String>>,
    failures: Mutex<Vec<String>>,
    audio: Mutex<Vec<(String, bool, u32)>>,
}

impl Backend {
    fn apply(&self, output: &str, path: &str) -> anyhow::Result<()> {
        skwd_wall_core::lock(&self.calls).push(output.to_owned());
        let mut failures = skwd_wall_core::lock(&self.failures);
        if let Some(index) = failures.iter().position(|failed| failed == output) {
            failures.remove(index);
            anyhow::bail!("backend unavailable");
        }
        self.state.renderers().set_assignment_for(output, path, &skwd_wall_core::outputs::names());
        Ok(())
    }
}

impl WallpaperApplication for Backend {
    fn release_outputs(&self, _: &[String]) -> anyhow::Result<()> {
        Ok(())
    }
    fn apply_static(&self, req: ApplyStaticRequest<'_>) -> anyhow::Result<()> {
        self.apply(req.output, req.path)
    }
    fn apply_static_smart(&self, req: StaticSmartRequest<'_>) -> anyhow::Result<()> {
        self.apply(req.output, req.path)
    }
    fn apply_video(&self, req: ApplyVideoRequest<'_>) -> anyhow::Result<()> {
        skwd_wall_core::lock(&self.audio).push((req.output.into(), req.mute, req.volume));
        self.apply(req.output, req.path)
    }
    fn apply_output(&self, req: ApplyOutputRequest<'_>) -> anyhow::Result<()> {
        skwd_wall_core::lock(&self.audio).push((req.output.into(), req.mute, req.volume));
        self.apply(req.output, req.path)
    }
    fn apply_video_transition(&self, _: VideoTransitionRequest<'_>) -> anyhow::Result<()> {
        self.apply("*", "")
    }
    fn apply_we(
        &self,
        _: &str,
        _: Option<OutputTransitionRequest<'_>>,
    ) -> anyhow::Result<Option<String>> {
        self.apply("*", "")?;
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
fn retains_successful_outputs() {
    let (_guard, root) = testenv::lock();
    testenv::write_config(serde_json::json!({"pickOnlyMode": false}));
    let image = root.join("retry.png");
    std::fs::write(&image, "fixture").unwrap();
    let (state, events, stats) = testenv::harness();
    let mut ctx = testenv::context(&state, &events, &stats);
    let backend = Arc::new(Backend {
        state: state.clone(),
        calls: Mutex::default(),
        failures: Mutex::new(vec!["DP-2".into()]),
        audio: Mutex::default(),
    });
    ctx.wallpaper = backend.clone();
    let prior = std::env::var_os("SKWD_FAKE_OUTPUTS");
    unsafe { std::env::set_var("SKWD_FAKE_OUTPUTS", "DP-1,DP-2") };
    let cache = state.config().cache_dir();
    skwd_wall_core::audio::write_state(
        &cache,
        &serde_json::json!({
            "DP-1": skwd_wall_core::audio::entry("static", image.to_str().unwrap(), "", true, 25),
            "DP-2": skwd_wall_core::audio::entry("static", image.to_str().unwrap(), "", false, 75),
        }),
    );
    std::fs::write(crate::infrastructure::restore_policy::monitors_path(), "{}").unwrap();
    let result =
        retry_restore(&[Duration::ZERO], || super::restore_pass(&ctx, ApplySource::Restore));
    unsafe {
        if let Some(prior) = prior {
            std::env::set_var("SKWD_FAKE_OUTPUTS", prior);
        } else {
            std::env::remove_var("SKWD_FAKE_OUTPUTS");
        }
    }
    result.unwrap();
    assert_eq!(*skwd_wall_core::lock(&backend.calls), ["DP-1", "DP-2", "DP-2"]);
}

#[test]
fn cancels_waiting_restore() {
    let (_guard, root) = testenv::lock();
    testenv::write_config(serde_json::json!({"pickOnlyMode": false}));
    let image = root.join("cancel.png");
    std::fs::write(&image, "fixture").unwrap();
    let (state, events, stats) = testenv::harness();
    let mut ctx = testenv::context(&state, &events, &stats);
    let backend = Arc::new(Backend {
        state: state.clone(),
        calls: Mutex::default(),
        failures: Mutex::default(),
        audio: Mutex::default(),
    });
    ctx.wallpaper = backend.clone();
    let apply = state.apply().lock();
    let started = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            started.wait();
            super::restore_last(&ctx, ApplySource::Restore);
        });
        started.wait();
        state.apply().cancel_restore(None);
        drop(apply);
    });
    assert!(skwd_wall_core::lock(&backend.calls).is_empty());
}

#[test]
fn preserves_output_audio() {
    let (_guard, root) = testenv::lock();
    testenv::write_config(serde_json::json!({"pickOnlyMode": false}));
    let video = root.join("restore-audio.mp4");
    std::fs::write(&video, "fixture").unwrap();
    let prior = std::env::var_os("SKWD_FAKE_OUTPUTS");
    unsafe { std::env::set_var("SKWD_FAKE_OUTPUTS", "DP-1,DP-2") };
    let mut observed = Vec::new();
    for (audio, wildcard) in [((true, 25), false), ((false, 75), false), ((false, 75), true)] {
        let (state, events, stats) = testenv::harness();
        let mut ctx = testenv::context(&state, &events, &stats);
        let backend = Arc::new(Backend {
            state: state.clone(),
            calls: Mutex::default(),
            failures: Mutex::default(),
            audio: Mutex::default(),
        });
        ctx.wallpaper = backend.clone();
        let cache = state.config().cache_dir();
        let mut recorded = serde_json::json!({
            "DP-1": skwd_wall_core::audio::entry("video", video.to_str().unwrap(), "", audio.0, audio.1),
            "DP-2": skwd_wall_core::audio::entry("video", video.to_str().unwrap(), "", false, 75),
        });
        if wildcard {
            recorded["*"] =
                skwd_wall_core::audio::entry("video", video.to_str().unwrap(), "", true, 25);
        }
        skwd_wall_core::audio::write_state(&cache, &recorded);
        std::fs::write(crate::infrastructure::restore_policy::monitors_path(), "{}").unwrap();
        super::restore_last(&ctx, ApplySource::Restore);
        observed.push(skwd_wall_core::lock(&backend.audio).clone());
    }
    unsafe {
        if let Some(prior) = prior {
            std::env::set_var("SKWD_FAKE_OUTPUTS", prior);
        } else {
            std::env::remove_var("SKWD_FAKE_OUTPUTS");
        }
    }
    assert_eq!(
        observed,
        [
            vec![("DP-1".into(), true, 25), ("DP-2".into(), false, 75)],
            vec![("*".into(), false, 75)],
            vec![("DP-1".into(), false, 75), ("DP-2".into(), false, 75)]
        ]
    );
}

#[test]
fn failed_hotplug_keeps_retry() {
    let (_guard, root) = testenv::lock();
    testenv::write_config(serde_json::json!({"pickOnlyMode": false}));
    let image = root.join("hotplug-retry.png");
    std::fs::write(&image, "fixture").unwrap();
    let (state, events, stats) = testenv::harness();
    let mut ctx = testenv::context(&state, &events, &stats);
    let backend = Arc::new(Backend {
        state: state.clone(),
        calls: Mutex::default(),
        failures: Mutex::new(vec!["DP-1".into(), "DP-1".into()]),
        audio: Mutex::default(),
    });
    ctx.wallpaper = backend.clone();
    let apply = |source| {
        crate::composition::apply::apply_core(
            &state,
            backend.as_ref(),
            ctx.history.as_ref(),
            ctx.events.as_ref(),
            &stats,
            wall_proto::kind::STATIC,
            image.to_str().unwrap(),
            "",
            true,
            0,
            source,
            "DP-1",
            true,
            false,
            None,
            None,
        )
    };
    let mut attempts = 0;
    let result = retry_restore(&[Duration::ZERO], || {
        attempts += 1;
        let result = apply(ApplySource::Restore).map(|_| ());
        if attempts == 1 {
            assert!(result.is_err());
            assert!(apply(ApplySource::Hotplug).is_err());
        }
        result
    });
    assert_eq!(attempts, 2);
    assert_eq!(*skwd_wall_core::lock(&backend.calls), ["DP-1", "DP-1", "DP-1"]);
    result.unwrap();
    assert_eq!(state.apply().generation(), 3);
}

#[test]
fn replans_changed_outputs() {
    let (_guard, root) = testenv::lock();
    testenv::write_config(serde_json::json!({"pickOnlyMode": false}));
    let image = root.join("changed-output.png");
    std::fs::write(&image, "fixture").unwrap();
    let (state, events, stats) = testenv::harness();
    let mut ctx = testenv::context(&state, &events, &stats);
    let backend = Arc::new(Backend {
        state: state.clone(),
        calls: Mutex::default(),
        failures: Mutex::new(vec!["DP-1".into()]),
        audio: Mutex::default(),
    });
    ctx.wallpaper = backend.clone();
    let cache = state.config().cache_dir();
    let entry = |mute, volume| {
        skwd_wall_core::audio::entry("static", image.to_str().unwrap(), "", mute, volume)
    };
    skwd_wall_core::audio::write_state(
        &cache,
        &serde_json::json!({
            "DP-1": entry(true, 25), "DP-2": entry(false, 75)
        }),
    );
    std::fs::write(crate::infrastructure::restore_policy::monitors_path(), "{}").unwrap();
    let prior = std::env::var_os("SKWD_FAKE_OUTPUTS");
    unsafe { std::env::set_var("SKWD_FAKE_OUTPUTS", "DP-1,DP-2") };
    assert!(super::restore_pass(&ctx, ApplySource::Restore).is_err());
    unsafe { std::env::set_var("SKWD_FAKE_OUTPUTS", "DP-2,DP-3") };
    skwd_wall_core::audio::set_entry(
        &cache,
        "DP-3",
        "static",
        image.to_str().unwrap(),
        "",
        true,
        25,
    );
    assert!(super::restore_pass(&ctx, ApplySource::Hotplug).is_ok());
    assert!(super::restore_pass(&ctx, ApplySource::Restore).is_ok());
    unsafe {
        if let Some(prior) = prior {
            std::env::set_var("SKWD_FAKE_OUTPUTS", prior);
        } else {
            std::env::remove_var("SKWD_FAKE_OUTPUTS");
        }
    }
    assert_eq!(
        skwd_wall_core::lock(&backend.calls).iter().filter(|output| *output == "DP-1").count(),
        1
    );
    assert!(skwd_wall_core::audio::read_state(&cache).get("DP-1").is_none());
    assert!(skwd_wall_core::lock(&backend.calls).iter().any(|output| output == "DP-3"));
}

#[test]
fn restores_returning_output() {
    let (_guard, root) = testenv::lock();
    testenv::write_config(serde_json::json!({"pickOnlyMode": false}));
    let image = root.join("returning.png");
    std::fs::write(&image, "fixture").unwrap();
    let (state, events, stats) = testenv::harness();
    let mut ctx = testenv::context(&state, &events, &stats);
    let backend = Arc::new(Backend {
        state: state.clone(),
        calls: Mutex::default(),
        failures: Mutex::new(vec!["DP-2".into(), "*".into()]),
        audio: Mutex::default(),
    });
    ctx.wallpaper = backend.clone();
    let cache = state.config().cache_dir();
    skwd_wall_core::audio::write_state(
        &cache,
        &serde_json::json!({
            "DP-1": skwd_wall_core::audio::entry("static", image.to_str().unwrap(), "", true, 25),
            "DP-2": skwd_wall_core::audio::entry("static", image.to_str().unwrap(), "", false, 75),
        }),
    );
    std::fs::write(crate::infrastructure::restore_policy::monitors_path(), "{}").unwrap();
    let prior = std::env::var_os("SKWD_FAKE_OUTPUTS");
    unsafe { std::env::set_var("SKWD_FAKE_OUTPUTS", "DP-1,DP-2") };
    assert!(super::restore_pass(&ctx, ApplySource::Restore).is_err());
    unsafe { std::env::set_var("SKWD_FAKE_OUTPUTS", "DP-2") };
    assert!(super::restore_pass(&ctx, ApplySource::Hotplug).is_err());
    unsafe { std::env::set_var("SKWD_FAKE_OUTPUTS", "DP-1,DP-2") };
    assert!(super::restore_pass(&ctx, ApplySource::Restore).is_ok());
    unsafe {
        if let Some(prior) = prior {
            std::env::set_var("SKWD_FAKE_OUTPUTS", prior);
        } else {
            std::env::remove_var("SKWD_FAKE_OUTPUTS");
        }
    }
    assert_eq!(*skwd_wall_core::lock(&backend.calls), ["DP-1", "DP-2", "*", "DP-1", "DP-2"]);
}
