use super::*;

#[cfg(unix)]
#[test]
fn weather_query_uses_relevance() {
    let directory = tempfile::tempdir().unwrap();
    let helper = directory.path().join("lens-helper");
    let request_path = directory.path().join("request.json");
    let script = format!(
        "#!/bin/sh\nread request\nprintf '%s\\n' \"$request\" > '{}'\nprintf '%s\\n' '{{\"generation\":0,\"queryMs\":1.0,\"searchMs\":1.0,\"matches\":[{{\"rank\":1,\"key\":\"static:first.png\",\"score\":0.5}},{{\"rank\":2,\"key\":\"static:second.png\",\"score\":0.4}}],\"error\":null}}'\n",
        request_path.display()
    );
    make_executable(&helper);
    std::fs::write(&helper, script).unwrap();
    let paths = SemanticPaths {
        helper,
        manifest: directory.path().join("manifest.json"),
        runtime: directory.path().join("runtime.so"),
        index: directory.path().join("index.sidx"),
        multiview: false,
    };
    let keys = search_weather_index(
        &paths,
        &WeatherQuery {
            query: "rainy skyline",
            negative: None,
            top_k: 8,
            score_window: 0.022,
            prominence: 0.015,
        },
    )
    .unwrap();
    assert_eq!(keys, ["static:first.png", "static:second.png"]);
    let request: SearchRequest =
        serde_json::from_slice(&std::fs::read(request_path).unwrap()).unwrap();
    assert_eq!(request.query, "rainy skyline");
    assert_eq!(request.top_k, 8);
    assert_eq!(request.score_window, Some(0.022));
    assert_eq!(request.min_score_prominence, Some(0.015));
    assert_eq!(request.max_results, None);
    assert_eq!(request.min_results, 0);
}

#[test]
fn weather_failure_retries() {
    let result = Err(String::from("helper unavailable"));
    let recent = (7, Instant::now(), result.clone());
    let expired = (
        7,
        Instant::now().checked_sub(crate::infrastructure::weather::refresh_interval()).unwrap(),
        result,
    );
    assert!(weather_cache_fresh(&recent, 7));
    assert!(!weather_cache_fresh(&expired, 7));
    assert!(!weather_cache_fresh(&recent, 8));
    let success = (7, expired.1, Ok(vec![String::from("static:one.png")]));
    assert!(weather_cache_fresh(&success, 7));
}

#[test]
fn preview_preserves_weather_cache() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({"weather":{"source":"semantic"}}));
    let state = Arc::new(WallState::open().unwrap());
    let previous = lock(&WEATHER_CACHE).take();
    *lock(&WEATHER_CACHE) =
        Some((17, Instant::now(), Ok(vec![String::from("static:current.png")])));
    assert!(weather_preview_search(&state, "rainy", 32, 0.022, 0.015).is_err());
    assert_eq!(
        lock(&WEATHER_CACHE).as_ref().unwrap().2,
        Ok(vec![String::from("static:current.png")])
    );
    *lock(&WEATHER_CACHE) = previous;
}

#[tokio::test]
async fn refresh_requests_coalesce() {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(1);
    assert!(tx.try_send(()).is_ok());
    for _ in 0..7 {
        let _ = tx.try_send(());
    }
    assert!(rx.recv().await.is_some());
    assert_eq!(rerun_or_idle(&mut rx), LoopStep::Idle);
    assert!(tx.try_send(()).is_ok());
    assert_eq!(rerun_or_idle(&mut rx), LoopStep::Rerun);
    assert_eq!(rerun_or_idle(&mut rx), LoopStep::Idle);
    drop(tx);
    assert_eq!(rerun_or_idle(&mut rx), LoopStep::Stopped);
}

#[test]
fn progress_uses_only_helper_reported_work() {
    let incremental = BuildProgress { progress: 3, total: 4, detail: String::from("three") };
    assert_eq!(normalized_progress(&incremental, 3_594), (3, 4));

    let oversized = BuildProgress { progress: 20, total: 12, detail: String::from("oversized") };
    assert_eq!(normalized_progress(&oversized, 10), (10, 10));
}

#[test]
fn catalog_fingerprints() {
    let rows = vec![
        serde_json::json!({"key":"static:a.png","name":"a.png","type":"static","thumb":"/t/a.webp","mtime":7}),
        serde_json::json!({"key":"shader:nope","name":"nope","type":"shader","thumb":"/t/nope.webp","mtime":8}),
        serde_json::json!({"key":"static:missing.png","name":"missing.png","type":"static","thumb":"","mtime":9}),
    ];
    let request = request_from_rows(&rows, false);
    assert_eq!(request.entries.len(), 1);
    assert_eq!(request.entries[0].key, "static:a.png");
    assert_eq!(request.entries[0].path, Path::new("/t/a.webp"));

    let changed = request_from_rows(
        &[
            serde_json::json!({"key":"static:a.png","name":"a.png","type":"static","thumb":"/t/a.webp","mtime":8}),
        ],
        false,
    );
    assert_ne!(request.fingerprint, changed.fingerprint);
    assert_ne!(request.entries[0].fingerprint, changed.entries[0].fingerprint);
}

#[test]
fn multiview_catalog_views() {
    let rows = vec![
        serde_json::json!({"key":"static:wide.png","name":"wide.png","type":"static","thumb":"/t/wide.webp","mtime":7,"width":3200,"height":1200}),
        serde_json::json!({"key":"video:tall.mp4","name":"tall.mp4","type":"video","thumb":"/t/tall.webp","mtime":8,"width":1080,"height":1920}),
    ];
    let request = request_from_rows(&rows, true);
    let views: Vec<ImageView> = request.entries.iter().map(|entry| entry.view).collect();
    assert_eq!(
        views,
        [
            ImageView::Full,
            ImageView::Center,
            ImageView::LeftThird,
            ImageView::RightThird,
            ImageView::Full,
            ImageView::Center,
        ]
    );
    assert_eq!(request.entries.iter().filter(|entry| entry.key == "static:wide.png").count(), 4);
    assert_eq!(request.entries.iter().filter(|entry| entry.key == "video:tall.mp4").count(), 2);
    assert_ne!(request.entries[0].fingerprint, request.entries[1].fingerprint);
}

#[test]
fn index_model_header() {
    let directory = tempfile::tempdir().unwrap();
    let index = directory.path().join("index.sidx");
    let mut bytes = Vec::from(b"SKWDSEM3".as_slice());
    bytes.extend_from_slice(&768_u32.to_le_bytes());
    bytes.extend_from_slice(&8_u32.to_le_bytes());
    bytes.extend_from_slice(b"model@v1");
    std::fs::write(&index, bytes).unwrap();
    assert_eq!(read_index_model(&index).as_deref(), Some("model@v1"));
}

#[test]
fn helper_prefers_canonical() {
    let directory = tempfile::tempdir().unwrap();
    let siblings = directory.path().join("siblings");
    let path_bin = directory.path().join("path-bin");
    std::fs::create_dir_all(&siblings).unwrap();
    std::fs::create_dir_all(&path_bin).unwrap();
    make_executable(&siblings.join("skwd-wall-semantic"));
    make_executable(&path_bin.join("skwd-lens"));
    let search_path = std::env::join_paths([&path_bin]).unwrap();

    let resolved = resolve_lens_helper(&siblings, None, Some(&search_path)).unwrap();

    assert_eq!(resolved, path_bin.join("skwd-lens"));
}

#[test]
fn helper_legacy_fallback() {
    let directory = tempfile::tempdir().unwrap();
    let siblings = directory.path().join("siblings");
    std::fs::create_dir_all(&siblings).unwrap();
    let legacy = siblings.join("skwd-wall-semantic");
    make_executable(&legacy);

    assert_eq!(resolve_lens_helper(&siblings, None, None), Some(legacy));
    assert!(resolve_lens_helper(&siblings, Some(directory.path().join("missing")), None).is_none());
}

#[cfg(unix)]
#[test]
fn helper_non_utf8_name() {
    use std::os::unix::ffi::OsStringExt;

    let directory = tempfile::tempdir().unwrap();
    let siblings = directory.path().join("siblings");
    std::fs::create_dir_all(&siblings).unwrap();
    let name = std::ffi::OsString::from_vec(b"skwd-lens-\xff".to_vec());
    let executable = siblings.join(&name);
    make_executable(&executable);

    assert_eq!(resolve_lens_helper(&siblings, Some(PathBuf::from(name)), None), Some(executable));
}

#[cfg(unix)]
#[test]
fn helper_skips_non_executable() {
    let directory = tempfile::tempdir().unwrap();
    let siblings = directory.path().join("siblings");
    let path_bin = directory.path().join("path-bin");
    std::fs::create_dir_all(&siblings).unwrap();
    std::fs::create_dir_all(&path_bin).unwrap();
    std::fs::write(siblings.join("skwd-lens"), b"").unwrap();
    let legacy = path_bin.join("skwd-wall-semantic");
    make_executable(&legacy);
    let search_path = std::env::join_paths([&path_bin]).unwrap();

    assert_eq!(resolve_lens_helper(&siblings, None, Some(&search_path)), Some(legacy));
}

#[test]
fn assets_prefer_canonical() {
    let directory = tempfile::tempdir().unwrap();
    let canonical = directory.path().join("skwd-lens/models/semantic");
    let sibling = directory.path().join("bin/lens");
    let legacy = directory.path().join("bin/semantic");
    for root in [&canonical, &sibling, &legacy] {
        make_semantic_assets(root);
    }

    let resolved =
        resolve_semantic_assets(&[canonical.clone(), sibling, legacy], None, None).unwrap();

    assert_eq!(resolved.0, canonical);
}

#[test]
fn assets_skip_incomplete() {
    let directory = tempfile::tempdir().unwrap();
    let canonical = directory.path().join("bin/lens");
    let legacy = directory.path().join("bin/semantic");
    std::fs::create_dir_all(&canonical).unwrap();
    std::fs::write(canonical.join("semantic-pack.json"), b"{}").unwrap();
    make_semantic_assets(&legacy);

    let resolved = resolve_semantic_assets(&[canonical, legacy.clone()], None, None).unwrap();

    assert_eq!(resolved.0, legacy);
}

#[test]
fn root_order() {
    let roots = default_semantic_roots(Path::new("/opt/skwd/bin"));

    assert!(roots[0].ends_with("skwd-lens/models/semantic"));
    assert_eq!(roots[1], Path::new("/opt/skwd/bin/lens"));
    assert!(roots[2].ends_with("skwd-wall-v2/models/semantic"));
    assert_eq!(roots[3], Path::new("/opt/skwd/bin/semantic"));
    assert_eq!(roots[4], Path::new("/opt/skwd/share/skwd-lens/models/semantic"));
}

#[test]
fn packaged_root_loses_to_user_root() {
    let directory = tempfile::tempdir().unwrap();
    let user = directory.path().join("skwd-lens/models/semantic");
    let packaged = directory.path().join("usr/share/skwd-lens/models/semantic");
    for root in [&user, &packaged] {
        make_semantic_assets(root);
    }

    let resolved = resolve_semantic_assets(&[user.clone(), packaged], None, None).unwrap();

    assert_eq!(resolved.0, user);
}

fn make_semantic_assets(root: &Path) {
    std::fs::create_dir_all(root.join("runtime")).unwrap();
    std::fs::write(root.join("semantic-pack.json"), b"{}").unwrap();
    std::fs::write(root.join("runtime/libonnxruntime.so"), b"").unwrap();
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

    std::fs::write(path, b"").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(not(unix))]
fn make_executable(path: &Path) {
    std::fs::write(path, b"").unwrap();
}

#[test]
fn empty_catalog_removes_index_and_fingerprint() {
    let directory = tempfile::tempdir().unwrap();
    let index = directory.path().join("index.sidx");
    let fingerprint = directory.path().join("index.sidx.fingerprint");
    std::fs::write(&index, b"old embeddings").unwrap();
    std::fs::write(&fingerprint, b"123").unwrap();
    clear_index(&index).unwrap();
    assert!(!index.exists());
    assert!(!fingerprint.exists());
    clear_index(&index).unwrap();
}

#[test]
fn response_limits_are_independent() {
    let responses: Vec<_> = (0..3)
        .map(|generation| SearchResponse::from_matches(generation, Vec::new(), 0.0, 0.0))
        .collect();
    let lines: Vec<_> =
        responses.iter().map(|response| serde_json::to_string(response).unwrap()).collect();
    let limit = lines[0].len() as u64 + 1;
    let wire = format!("{}\n", lines.join("\n"));
    assert!(wire.len() as u64 > limit);
    let decoded =
        read_responses(&mut wire.as_bytes(), &[0, 1, 2], Vec::new(), Vec::push, limit).unwrap();
    assert_eq!(decoded, responses);
    let oversized =
        read_responses(&mut wire.as_bytes(), &[0, 1, 2], Vec::new(), Vec::push, limit - 1)
            .unwrap_err();
    assert_eq!(oversized, WEATHER_RESPONSE_TOO_LARGE);
}

#[test]
fn response_generations_are_checked() {
    let response = SearchResponse::from_matches(7, Vec::new(), 0.0, 0.0);
    let wire = format!("{}\n", serde_json::to_string(&response).unwrap());
    let wrong =
        read_responses(&mut wire.as_bytes(), &[8], Vec::new(), Vec::push, WEATHER_RESPONSE_BYTES)
            .unwrap_err();
    assert_eq!(wrong, WEATHER_GENERATION_MISMATCH);
    let extra = read_responses(
        &mut wire.repeat(2).as_bytes(),
        &[7],
        Vec::new(),
        Vec::push,
        WEATHER_RESPONSE_BYTES,
    )
    .unwrap_err();
    assert_eq!(extra, WEATHER_GENERATION_MISMATCH);
    assert!(
        read_responses(
            &mut wire.as_bytes(),
            &[7, 8],
            Vec::new(),
            Vec::push,
            WEATHER_RESPONSE_BYTES,
        )
        .is_err()
    );
}
