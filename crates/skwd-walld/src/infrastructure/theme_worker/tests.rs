#![cfg(test)]

use std::time::Duration;

use tokio::time::Instant;

use super::{ThemeJob, next_ready};

#[tokio::test(start_paused = true)]
async fn slot_latest_wins() {
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    sender
        .send(ThemeJob {
            path: "/w/a.png".into(),
            ready_at: Instant::now() + Duration::from_secs(1),
            weather_generation: None,
        })
        .unwrap();
    sender
        .send(ThemeJob {
            path: "/w/b.png".into(),
            ready_at: Instant::now(),
            weather_generation: None,
        })
        .unwrap();
    let first = receiver.recv().await.unwrap();
    let job = next_ready(first, &mut receiver).await;
    assert_eq!(job.path, "/w/b.png");
    assert!(job.ready_at <= Instant::now());
    assert!(receiver.try_recv().is_err());
}

#[tokio::test(start_paused = true)]
async fn delayed_job_waits_until_ready() {
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let t0 = Instant::now();
    sender
        .send(ThemeJob {
            path: "/w/a.png".into(),
            ready_at: t0 + Duration::from_secs(300),
            weather_generation: None,
        })
        .unwrap();
    let first = receiver.recv().await.unwrap();
    let job = next_ready(first, &mut receiver).await;
    assert_eq!(job.path, "/w/a.png");
    assert!(Instant::now() >= t0 + Duration::from_secs(300));
}

#[tokio::test]
async fn delay_real_time() {
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let t0 = Instant::now();
    sender
        .send(ThemeJob {
            path: "/w/a.png".into(),
            ready_at: t0 + Duration::from_millis(30),
            weather_generation: None,
        })
        .unwrap();
    let first = receiver.recv().await.unwrap();
    let job = next_ready(first, &mut receiver).await;
    assert_eq!(job.path, "/w/a.png");
    assert!(t0.elapsed() >= Duration::from_millis(30));
}

#[test]
fn stale_weather_jobs_cancel() {
    let (_guard, _root) = crate::testenv::lock();
    for change in ["disabled", "stopped", "manual", "source"] {
        crate::testenv::write_config(serde_json::json!({
            "weather": {"enabled": true, "source": "outdoors", "outdoors": {"retheme": true}}
        }));
        let (state, events, _) = crate::testenv::harness();
        state.theme().set_source("/weather.jpg");
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let job = ThemeJob {
            path: "/weather.jpg".into(),
            ready_at: Instant::now(),
            weather_generation: Some(state.theme().source_generation()),
        };
        assert!(job.current(&state));
        sender.send(job).unwrap();
        match change {
            "disabled" | "stopped" => {
                crate::testenv::write_config(serde_json::json!({
                    "weather": {"enabled": change != "stopped", "source": "outdoors",
                        "outdoors": {"retheme": change != "disabled"}}
                }));
            }
            "manual" => {
                state.theme().set_source("/weather.jpg");
            }
            _ => state.theme().set_source("/manual.jpg"),
        }
        let job = receiver.try_recv().unwrap();
        let preview = state.theme().shell_preview_generation();
        super::apply_theme(&state, events.as_ref(), &job);
        assert_eq!(state.theme().shell_preview_generation(), preview);
        assert!(skwd_wall_core::theme::profiles::current(&state).is_err());
        let manual = ThemeJob { weather_generation: None, ..job };
        assert!(manual.current(&state));
    }
}

#[test]
fn other_output_preserves_theme() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({
        "weather": {"enabled": true, "source": "outdoors", "outdoors": {"retheme": true}},
        "display": {"themeOutput": "DP-1"}
    }));
    let (state, _, _) = crate::testenv::harness();
    state.theme().set_source("/weather.jpg");
    let job = ThemeJob {
        path: "/weather.jpg".into(),
        ready_at: Instant::now(),
        weather_generation: Some(state.theme().source_generation()),
    };
    state.apply().next_generation();
    assert!(job.current(&state));
    state.apply().next_generation();
    assert!(job.current(&state));
}

#[test]
fn skipped_theme_invalidates_weather() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({
        "weather": {"enabled": true, "source": "outdoors", "outdoors": {"retheme": true}}
    }));
    let (state, _, _) = crate::testenv::harness();
    state.theme().set_source("/weather.jpg");
    let job = ThemeJob {
        path: "/weather.jpg".into(),
        ready_at: Instant::now(),
        weather_generation: Some(state.theme().source_generation()),
    };
    assert!(job.current(&state));
    state.theme().invalidate_source();
    assert_eq!(state.theme().source().as_deref(), Some("/weather.jpg"));
    assert!(!job.current(&state));
    let manual = ThemeJob { weather_generation: None, ..job };
    assert!(manual.current(&state));
}

#[test]
fn valid_jobs_publish_palette() {
    let (_guard, _root) = crate::testenv::lock();
    let palette: serde_json::Map<String, serde_json::Value> =
        skwd_wall_core::theme::profiles::ROLE_KEYS
            .into_iter()
            .map(|key| (key.into(), serde_json::json!("#123456")))
            .collect();
    crate::testenv::write_config(serde_json::json!({
        "weather": {"enabled": true, "source": "outdoors", "outdoors": {"retheme": true}},
        "theme": {"policy": "wallpaper", "mode": "dark", "wallpaperProfiles": [
            {"key": "/weather.jpg", "enabled": true, "dark": palette}
        ]}
    }));
    let (state, events, _) = crate::testenv::harness();
    std::fs::create_dir_all(state.config().cache_dir()).unwrap();
    assert_eq!(
        skwd_wall_core::theme::profiles::palette(&state, "/weather.jpg"),
        Some(serde_json::Value::Object(palette))
    );
    for weather in [false, true] {
        state.theme().set_source("/weather.jpg");
        let job = ThemeJob {
            path: "/weather.jpg".into(),
            ready_at: Instant::now(),
            weather_generation: weather.then(|| state.theme().source_generation()),
        };
        let state = std::sync::Arc::clone(&state);
        let events = std::sync::Arc::clone(&events);
        let (sender, receiver) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            super::apply_theme(&state, events.as_ref(), &job);
            sender.send(skwd_wall_core::theme::profiles::current(&state)).unwrap();
        });
        let applied = receiver.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
        worker.join().unwrap();
        assert_eq!(applied["palette"]["primary"], "#123456");
        assert_eq!(applied["source"], "/weather.jpg");
    }
}
