use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::*;

fn response(id: u64, generation: u64) -> Response {
    Response::ok(
        id,
        json!(wall_proto::sources::ListResult {
            generation: Some(generation),
            results: vec![wall_proto::sources::ListItem {
                id: format!("item-{generation}"),
                ..wall_proto::sources::ListItem::default()
            }],
            ..wall_proto::sources::ListResult::default()
        }),
    )
}

fn stats() -> Arc<Stats> {
    Arc::new(Stats::new())
}

fn preview_request(refresh: bool) -> Request {
    Request { id: 1, method: "source.preview".into(), params: json!({"refresh": refresh}) }
}

#[test]
fn details_detached() {
    let request = Request {
        id: 1,
        method: wall_proto::rpc::SOURCE_DETAILS.into(),
        params: json!({"source": "steam", "id": "123", "generation": 4}),
    };
    let call = list_call(&request).unwrap().unwrap();
    assert_eq!(call.provider, wall_proto::sources::Provider::Steam);
    assert_eq!(call.generation, None);
}

#[test]
fn preview_failure_emits_event() {
    let events = Arc::new(EventHub::new(stats()));
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    events.subscribe(1, sender);
    let directory = tempfile::tempdir().unwrap();
    let response = handle_remote_preview(
        &preview_request(false),
        "steam",
        "123",
        "https://attacker.example/image.png".into(),
        directory.path().join("image.png"),
        &events,
        |url| crate::infrastructure::http::require_source("steam", url),
    );
    assert_eq!(response.result.unwrap()["status"], "fetching");
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let event = loop {
        if let Ok(message) = receiver.try_recv() {
            break serde_json::from_str::<wall_proto::Event>(&message).unwrap();
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(event.event, ev::PREVIEW_READY);
    assert_eq!(event.data["id"], "123");
    assert_eq!(event.data["source"], "steam");
    assert_eq!(event.data["path"], "");
    assert!(event.data["error"].as_str().unwrap().contains("blocked"));
}

#[test]
fn cached_preview_is_ready() {
    let events = Arc::new(EventHub::new(stats()));
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("image.png");
    image::RgbaImage::from_pixel(2, 2, image::Rgba([1, 2, 3, 255])).save(&path).unwrap();
    let response = handle_remote_preview(
        &preview_request(false),
        "steam",
        "123",
        "https://images.steamusercontent.com/image.png".into(),
        path.clone(),
        &events,
        |_| Ok(()),
    );
    let result = response.result.unwrap();
    assert_eq!(result["status"], "ready");
    assert_eq!(result["path"], path.to_string_lossy().as_ref());
}

#[test]
fn retry_discards_truncated_cache() {
    let events = Arc::new(EventHub::new(stats()));
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    events.subscribe(1, sender);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("image.png");
    image::RgbaImage::from_pixel(2, 2, image::Rgba([1, 2, 3, 255])).save(&path).unwrap();
    let len = std::fs::metadata(&path).unwrap().len();
    std::fs::OpenOptions::new().write(true).open(&path).unwrap().set_len(len - 8).unwrap();
    assert!(crate::infrastructure::http::validate_cached_preview_header(&path).is_ok());
    assert!(crate::infrastructure::http::validate_cached_preview(&path).is_err());

    let url = "https://attacker.example/image.png";
    let response = handle_remote_preview(
        &preview_request(false),
        "steam",
        "123",
        url.into(),
        path.clone(),
        &events,
        |_| Ok(()),
    );
    assert_eq!(response.result.unwrap()["status"], "ready");
    assert!(receiver.try_recv().is_ok());
    let response = handle_remote_preview(
        &preview_request(true),
        "steam",
        "123",
        url.into(),
        path.clone(),
        &events,
        |url| crate::infrastructure::http::require_source("steam", url),
    );
    assert_eq!(response.result.unwrap()["status"], "fetching");
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while path.exists() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut failed = false;
    while !failed {
        let event = receiver
            .try_recv()
            .ok()
            .and_then(|message| serde_json::from_str::<wall_proto::Event>(&message).ok());
        failed = event.is_some_and(|event| !event.data["error"].as_str().unwrap_or("").is_empty());
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn provider_list_contract_success_error_and_cancel() {
    crate::testenv::runtime().block_on(async {
        for provider in [
            wall_proto::sources::Provider::Wallhaven,
            wall_proto::sources::Provider::Steam,
            wall_proto::sources::Provider::Unsplash,
        ] {
            let lifecycle = Arc::new(ListLifecycle::default());
            let success = run_list(
                Arc::clone(&lifecycle),
                ListCall { provider, generation: Some(1) },
                1,
                Duration::from_secs(1),
                stats(),
                || response(1, 1),
            )
            .await;
            let result: wall_proto::sources::ListResult =
                serde_json::from_value(success.result.unwrap()).unwrap();
            assert_eq!(result.generation, Some(1));
            assert_eq!(result.results[0].id, "item-1");

            let failure = run_list(
                Arc::clone(&lifecycle),
                ListCall { provider, generation: Some(2) },
                2,
                Duration::from_secs(1),
                stats(),
                || Response::err(2, -7, "provider rejected request"),
            )
            .await;
            assert_eq!(failure.error.unwrap().code, -7);

            lifecycle.observe(provider, 4);
            let ran = Arc::new(AtomicBool::new(false));
            let worker_ran = Arc::clone(&ran);
            let cancelled = run_list(
                Arc::clone(&lifecycle),
                ListCall { provider, generation: Some(3) },
                3,
                Duration::from_secs(1),
                stats(),
                move || {
                    worker_ran.store(true, Ordering::Release);
                    response(3, 3)
                },
            )
            .await;
            assert_eq!(cancelled.error.unwrap().code, LIST_CANCELLED);
            assert!(!ran.load(Ordering::Acquire));
        }
    });
}

#[test]
fn provider_list_contract_cancels_before_publication() {
    crate::testenv::runtime().block_on(async {
        for provider in [
            wall_proto::sources::Provider::Wallhaven,
            wall_proto::sources::Provider::Steam,
            wall_proto::sources::Provider::Pexels,
        ] {
            let lifecycle = Arc::new(ListLifecycle::default());
            let task_lifecycle = Arc::clone(&lifecycle);
            let (started_tx, started_rx) = tokio::sync::oneshot::channel();
            let (release_tx, release_rx) = std::sync::mpsc::channel();
            let task = tokio::spawn(async move {
                run_list(
                    task_lifecycle,
                    ListCall { provider, generation: Some(8) },
                    8,
                    Duration::from_secs(1),
                    stats(),
                    move || {
                        let _ = started_tx.send(());
                        release_rx.recv().unwrap();
                        response(8, 8)
                    },
                )
                .await
            });
            started_rx.await.unwrap();
            lifecycle.observe(provider, 9);
            let cancelled = task.await.unwrap();
            assert_eq!(cancelled.error.unwrap().code, LIST_CANCELLED);
            release_tx.send(()).unwrap();
        }
    });
}

#[test]
fn provider_list_contract_times_out() {
    crate::testenv::runtime().block_on(async {
        for provider in [
            wall_proto::sources::Provider::Wallhaven,
            wall_proto::sources::Provider::Steam,
            wall_proto::sources::Provider::Youtube,
        ] {
            let timed_out = run_list(
                Arc::new(ListLifecycle::default()),
                ListCall { provider, generation: Some(1) },
                1,
                Duration::from_millis(5),
                stats(),
                || {
                    std::thread::sleep(Duration::from_millis(30));
                    response(1, 1)
                },
            )
            .await;
            assert_eq!(timed_out.error.unwrap().code, LIST_TIMEOUT);
        }
    });
}

#[test]
fn replacement_provider_cancels_previous_provider() {
    crate::testenv::runtime().block_on(async {
        let lifecycle = Arc::new(ListLifecycle::default());
        lifecycle.observe(wall_proto::sources::Provider::Wallhaven, 10);
        let current = run_list(
            Arc::clone(&lifecycle),
            ListCall { provider: wall_proto::sources::Provider::Steam, generation: Some(11) },
            11,
            Duration::from_secs(1),
            stats(),
            || response(11, 11),
        )
        .await;
        assert!(current.error.is_none());

        let stale = run_list(
            lifecycle,
            ListCall { provider: wall_proto::sources::Provider::Wallhaven, generation: Some(10) },
            10,
            Duration::from_secs(1),
            stats(),
            || response(10, 10),
        )
        .await;
        assert_eq!(stale.error.unwrap().code, LIST_CANCELLED);
    });
}

#[test]
fn list_call_rejects_unknown_and_malformed_generation() {
    let unknown = Request {
        method: wall_proto::rpc::SOURCE_LIST.into(),
        params: json!({"source": "future", "generation": 1}),
        id: 7,
    };
    assert_eq!(list_call(&unknown).unwrap_err().error.unwrap().code, -32602);

    let malformed = Request {
        method: wall_proto::rpc::SOURCE_LIST.into(),
        params: json!({"source": "wallhaven", "generation": "new"}),
        id: 8,
    };
    assert_eq!(list_call(&malformed).unwrap_err().error.unwrap().code, -32602);

    let future_method =
        Request { method: String::from("future.search"), params: json!({"generation": 1}), id: 9 };
    assert!(list_call(&future_method).unwrap().is_none());
}
