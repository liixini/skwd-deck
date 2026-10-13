use std::collections::HashSet;
use std::ffi::OsStr;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use skwd_wall_core::infrastructure::config::ConfigStore;
use skwd_wall_core::infrastructure::database::Database;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::{Receiver, Sender};
use wall_proto::{TaskCapabilities, TaskControl, TaskState, TaskStatus, ev};

use crate::backend::events::EventPublisher;
use crate::infrastructure::events::EventHub;
use crate::infrastructure::tasks::TaskRegistry;

use skwd_lens_proto::{
    BuildEntry, BuildProgress, BuildRequest, ImageView, SearchRequest, SearchResponse,
    cache_index_name, manifest_identity,
};
use skwd_wall_core::{WallState, lock};

mod outdoors;

static REFRESH: OnceLock<Sender<()>> = OnceLock::new();
static SETTINGS_CHANGED: tokio::sync::Notify = tokio::sync::Notify::const_new();

static CONTROL: OnceLock<Sender<TaskControl>> = OnceLock::new();
type WeatherCacheEntry = (u64, Instant, Result<Vec<String>, String>);
type WeatherSearchSnapshot = (Option<SemanticPaths>, Option<BuildRequest>, u64);
static WEATHER_CACHE: Mutex<Option<WeatherCacheEntry>> = Mutex::new(None);
const WEATHER_SEARCH_TIMEOUT: Duration = Duration::from_secs(30);
const WEATHER_SEARCH_WAIT: Duration = Duration::from_millis(25);
const WEATHER_SEARCH_THREADS: &str = "2";
const WEATHER_RESPONSE_BYTES: u64 = 33_554_432;
const LENS_SERVE_ARG: &str = "--serve";
const LENS_MANIFEST_ARG: &str = "--manifest";
const LENS_INDEX_ARG: &str = "--index";
const LENS_RUNTIME_ARG: &str = "--runtime";
const LENS_THREADS_ARG: &str = "--threads";
const SEARCH_LINE_END: &[u8] = b"\n";
const WEATHER_INDEX_UNAVAILABLE: &str = "semantic index unavailable";
const WEATHER_SEARCH_SUPERSEDED: &str = "semantic search superseded";
const WEATHER_SEARCH_TIMED_OUT: &str = "semantic search timed out";
const WEATHER_RESPONSE_TOO_LARGE: &str = "semantic response exceeds size limit";
const WEATHER_READER_FAILED: &str = "semantic reader failed";
const WEATHER_GENERATION_MISMATCH: &str = "semantic response generation mismatch";

struct SemanticPaths {
    helper: PathBuf,
    manifest: PathBuf,
    runtime: PathBuf,
    index: PathBuf,
    multiview: bool,
}

type RefreshSnapshot = (SemanticPaths, BuildRequest, bool);

pub(crate) fn start(
    config: Arc<ConfigStore>,
    database: Arc<Database>,
    events: Arc<EventHub>,
    tasks: Arc<TaskRegistry>,
) {
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    let (control_tx, control_rx) = tokio::sync::mpsc::channel(8);
    if REFRESH.set(tx).is_err() {
        return;
    }
    let _ = CONTROL.set(control_tx);
    tokio::spawn(refresh_loop(rx, control_rx, config, database, events, tasks));
}

pub(crate) fn settings_changed() {
    SETTINGS_CHANGED.notify_one();
    request_refresh();
}

pub(crate) fn request_refresh() {
    if let Some(tx) = REFRESH.get() {
        let _ = tx.try_send(());
    }
}

pub(crate) fn control(action: TaskControl) -> bool {
    CONTROL.get().is_some_and(|sender| sender.try_send(action).is_ok())
}

#[derive(Debug, PartialEq, Eq)]
enum LoopStep {
    Rerun,
    Idle,
    Stopped,
}

fn rerun_or_idle(refreshes: &mut Receiver<()>) -> LoopStep {
    match refreshes.try_recv() {
        Ok(()) => LoopStep::Rerun,
        Err(TryRecvError::Empty) => LoopStep::Idle,
        Err(TryRecvError::Disconnected) => LoopStep::Stopped,
    }
}

async fn refresh_loop(
    mut refreshes: Receiver<()>,
    mut controls: Receiver<TaskControl>,
    config: Arc<ConfigStore>,
    database: Arc<Database>,
    events: Arc<EventHub>,
    tasks: Arc<TaskRegistry>,
) {
    while refreshes.recv().await.is_some() {
        loop {
            let result = tokio::select! {
                biased;
                () = SETTINGS_CHANGED.notified() => {
                    tasks.finish_if_active("semantic-index", TaskState::Cancelled, "Search settings changed");
                    continue;
                }
                result = refresh(&config, &database, &events, &tasks, &mut controls) => result,
            };
            if let Err(error) = result {
                log::warn!("semantic index refresh failed: {error}");
                if error.to_string() != "semantic index cancelled" {
                    tasks.finish("semantic-index", TaskState::Failed, error.to_string());
                }
            }
            match rerun_or_idle(&mut refreshes) {
                LoopStep::Rerun => {}
                LoopStep::Idle => break,
                LoopStep::Stopped => return,
            }
        }
    }
}

async fn refresh(
    config: &Arc<ConfigStore>,
    database: &Arc<Database>,
    events: &Arc<EventHub>,
    tasks: &Arc<TaskRegistry>,
    controls: &mut Receiver<TaskControl>,
) -> anyhow::Result<()> {
    if !config.read().semantic_enabled() {
        return Ok(());
    }
    let snapshot_config = Arc::clone(config);
    let snapshot_database = Arc::clone(database);
    let snapshot = tokio::task::spawn_blocking(move || -> anyhow::Result<RefreshSnapshot> {
        let paths = discover_paths(&snapshot_config)
            .ok_or_else(|| anyhow::anyhow!("semantic pack unavailable"))?;
        let request = catalog_request(&snapshot_database, paths.multiview)?;
        let rebuilt = !index_current(&paths, request.fingerprint);
        Ok((paths, request, rebuilt))
    })
    .await??;
    let (paths, request, rebuilt) = snapshot;
    if request.entries.is_empty() {
        let index = paths.index.clone();
        tokio::task::spawn_blocking(move || clear_index(&index)).await??;
        events.publish(
            ev::SEMANTIC_INDEX_READY,
            serde_json::json!({"items": 0, "fingerprint": request.fingerprint}),
        );
        crate::composition::runtime::weather::reload();
        return Ok(());
    }
    if rebuilt {
        let mut task = TaskStatus::running("semantic-index", "index", "Updating search index");
        task.detail = String::from("Checking cached embeddings");
        task.capabilities.pause = true;
        task.capabilities.stop = true;
        tasks.update(task);
        build_index(&paths, &request, controls, tasks).await?;
        tasks.finish("semantic-index", TaskState::Completed, "Search index ready");
    }
    if rebuilt {
        events.publish(
            ev::SEMANTIC_INDEX_READY,
            serde_json::json!({"items": request.entries.len(), "fingerprint": request.fingerprint}),
        );
        crate::composition::runtime::weather::reload();
    }
    Ok(())
}

pub(crate) fn weather_search(
    state: &Arc<WallState>,
    query: &str,
    max_results: usize,
    score_window: f32,
    min_score_prominence: f32,
) -> Result<Vec<String>, String> {
    let (paths, catalog, signature) =
        weather_snapshot(state, query, max_results, score_window, min_score_prominence);
    if let Some(entry) = lock(&WEATHER_CACHE).as_ref()
        && weather_cache_fresh(entry, signature)
    {
        return entry.2.clone();
    }
    let result = checked_weather_search(
        state,
        query,
        max_results,
        score_window,
        min_score_prominence,
        (paths, catalog, signature),
    );
    *lock(&WEATHER_CACHE) = Some((signature, Instant::now(), result.clone()));
    result
}

pub(crate) fn outdoors_search(
    state: &Arc<WallState>,
    context: &wall_rules::outdoors::Context,
    resolve: bool,
) -> Option<Result<Vec<String>, String>> {
    let config = state.config().clone();
    let query = context.query();
    let negative = context.negative_query();
    let identity = format!("{query}\n{negative}\n{}", config.outdoors_contrast_margin());
    let snapshot = || {
        weather_snapshot(
            state,
            &identity,
            config.weather_semantic_max_results(),
            config.weather_semantic_score_window(),
            config.weather_semantic_prominence(),
        )
    };
    let (paths, catalog, signature) = snapshot();
    if let Some(entry) = lock(&WEATHER_CACHE).as_ref()
        && weather_cache_fresh(entry, signature)
    {
        return Some(entry.2.clone());
    }
    if !resolve {
        return None;
    }
    let result = match (paths, catalog) {
        (Some(paths), Some(catalog))
            if config.semantic_enabled() && index_current(&paths, catalog.fingerprint) =>
        {
            let count =
                catalog.entries.iter().map(|item| item.key.as_str()).collect::<HashSet<_>>().len();
            outdoors::search(&paths, context, count, config.outdoors_contrast_margin())
        }
        _ => Err(String::from(WEATHER_INDEX_UNAVAILABLE)),
    };
    state.reload_config();
    if snapshot().2 != signature {
        return Some(Err(String::from(WEATHER_SEARCH_SUPERSEDED)));
    }
    *lock(&WEATHER_CACHE) = Some((signature, Instant::now(), result.clone()));
    Some(result)
}

pub(crate) fn weather_preview_search(
    state: &Arc<WallState>,
    query: &str,
    max_results: usize,
    score_window: f32,
    min_score_prominence: f32,
) -> Result<Vec<String>, String> {
    let snapshot = weather_snapshot(state, query, max_results, score_window, min_score_prominence);
    checked_weather_search(state, query, max_results, score_window, min_score_prominence, snapshot)
}

fn checked_weather_search(
    state: &Arc<WallState>,
    query: &str,
    max_results: usize,
    score_window: f32,
    min_score_prominence: f32,
    (paths, catalog, signature): WeatherSearchSnapshot,
) -> Result<Vec<String>, String> {
    let result =
        weather_search_snapshot(state, query, score_window, min_score_prominence, paths, catalog);
    state.reload_config();
    let (_, _, current) =
        weather_snapshot(state, query, max_results, score_window, min_score_prominence);
    if current != signature {
        return Err(String::from(WEATHER_SEARCH_SUPERSEDED));
    }
    result
}

fn weather_search_snapshot(
    state: &Arc<WallState>,
    query: &str,
    score_window: f32,
    min_score_prominence: f32,
    paths: Option<SemanticPaths>,
    catalog: Option<BuildRequest>,
) -> Result<Vec<String>, String> {
    match (paths, catalog) {
        (Some(paths), Some(catalog))
            if state.config().semantic_enabled() && index_current(&paths, catalog.fingerprint) =>
        {
            let unique = catalog
                .entries
                .iter()
                .map(|entry| entry.key.as_str())
                .collect::<HashSet<_>>()
                .len();
            search_weather_index(
                &paths,
                &WeatherQuery {
                    query,
                    negative: None,
                    top_k: unique,
                    score_window,
                    prominence: min_score_prominence,
                },
            )
        }
        _ => Err(String::from(WEATHER_INDEX_UNAVAILABLE)),
    }
}

pub(crate) fn weather_cached(
    state: &Arc<WallState>,
    query: &str,
    max_results: usize,
    score_window: f32,
    min_score_prominence: f32,
) -> Option<Result<Vec<String>, String>> {
    let (_, _, signature) =
        weather_snapshot(state, query, max_results, score_window, min_score_prominence);
    lock(&WEATHER_CACHE)
        .as_ref()
        .and_then(|entry| weather_cache_fresh(entry, signature).then(|| entry.2.clone()))
}

fn weather_cache_fresh(entry: &WeatherCacheEntry, signature: u64) -> bool {
    entry.0 == signature
        && (entry.2.is_ok()
            || entry.1.elapsed() < crate::infrastructure::weather::refresh_interval())
}

pub(crate) fn clear_failed_weather_search() {
    let mut cache = lock(&WEATHER_CACHE);
    if cache.as_ref().is_some_and(|(_, _, result)| result.is_err()) {
        *cache = None;
    }
}

pub(crate) fn weather_cached_match(
    state: &Arc<WallState>,
    query: &str,
    max_results: usize,
    score_window: f32,
    min_score_prominence: f32,
    key: &str,
) -> bool {
    weather_cached(state, query, max_results, score_window, min_score_prominence)
        .is_some_and(|result| result.is_ok_and(|keys| keys.iter().any(|member| member == key)))
}

fn weather_snapshot(
    state: &Arc<WallState>,
    query: &str,
    max_results: usize,
    score_window: f32,
    min_score_prominence: f32,
) -> (Option<SemanticPaths>, Option<BuildRequest>, u64) {
    let paths = discover_paths(&state.config_store());
    let catalog =
        paths.as_ref().and_then(|paths| catalog_request(&state.database(), paths.multiview).ok());
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    query.hash(&mut hasher);
    max_results.hash(&mut hasher);
    score_window.to_bits().hash(&mut hasher);
    min_score_prominence.to_bits().hash(&mut hasher);
    state.config().semantic_enabled().hash(&mut hasher);
    if let Some(paths) = &paths {
        for path in [&paths.helper, &paths.manifest, &paths.runtime, &paths.index] {
            path.hash(&mut hasher);
            if let Ok(meta) = std::fs::metadata(path) {
                meta.len().hash(&mut hasher);
                meta.modified().ok().hash(&mut hasher);
            }
        }
        let sidecar = PathBuf::from(format!("{}.fingerprint", paths.index.display()));
        std::fs::read(sidecar).ok().hash(&mut hasher);
    }
    catalog.as_ref().map(|request| request.fingerprint).hash(&mut hasher);
    (paths, catalog, hasher.finish())
}

struct WeatherQuery<'a> {
    query: &'a str,
    negative: Option<&'a str>,
    top_k: usize,
    score_window: f32,
    prominence: f32,
}

fn search_weather_index(
    paths: &SemanticPaths,
    spec: &WeatherQuery<'_>,
) -> Result<Vec<String>, String> {
    let request = SearchRequest {
        generation: 0,
        query: spec.query.to_owned(),
        negative_query: spec.negative.map(str::to_owned),
        negative_weight: 0.5,
        top_k: spec.top_k,
        score_window: Some(spec.score_window),
        min_score_prominence: Some(spec.prominence),
        max_results: None,
        min_results: 0,
        embedding_only: false,
    };
    search_requests(paths, &[request], Vec::new(), |keys, response| {
        keys.extend(response.matches.into_iter().map(|item| item.key));
    })
}

pub(crate) fn available(state: &Arc<WallState>) -> bool {
    discover_paths(&state.config_store()).is_some()
}

fn search_requests<T: Send + 'static>(
    paths: &SemanticPaths,
    requests: &[SearchRequest],
    initial: T,
    consume: fn(&mut T, SearchResponse),
) -> Result<T, String> {
    let args = [
        OsStr::new(LENS_SERVE_ARG),
        OsStr::new(LENS_MANIFEST_ARG),
        paths.manifest.as_os_str(),
        OsStr::new(LENS_INDEX_ARG),
        paths.index.as_os_str(),
        OsStr::new(LENS_RUNTIME_ARG),
        paths.runtime.as_os_str(),
        OsStr::new(LENS_THREADS_ARG),
        OsStr::new(WEATHER_SEARCH_THREADS),
    ];
    let mut child = crate::infrastructure::proc::tool(&paths.helper)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| error.to_string())?;
    let stdout = child.stdout.take().ok_or("semantic stdout unavailable")?;
    let generations: Vec<_> = requests.iter().map(|request| request.generation).collect();
    let reader = std::thread::spawn(move || {
        read_responses(
            &mut std::io::BufReader::new(stdout),
            &generations,
            initial,
            consume,
            WEATHER_RESPONSE_BYTES,
        )
    });
    let result = (|| {
        let mut stdin = child.stdin.take().ok_or("semantic stdin unavailable")?;
        for request in requests {
            serde_json::to_writer(&mut stdin, request).map_err(|error| error.to_string())?;
            stdin.write_all(SEARCH_LINE_END).map_err(|error| error.to_string())?;
        }
        drop(stdin);
        let deadline = Instant::now() + WEATHER_SEARCH_TIMEOUT;
        loop {
            if child.try_wait().map_err(|error| error.to_string())?.is_some() {
                break;
            }
            if Instant::now() >= deadline {
                return Err(String::from(WEATHER_SEARCH_TIMED_OUT));
            }
            std::thread::sleep(WEATHER_SEARCH_WAIT);
        }
        let status = child.wait().map_err(|error| error.to_string())?;
        if !status.success() {
            return Err(format!("semantic search exited with {status}"));
        }
        reader.join().map_err(|_| String::from(WEATHER_READER_FAILED))?
    })();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

fn read_responses<T>(
    reader: &mut impl BufRead,
    generations: &[u64],
    mut result: T,
    consume: fn(&mut T, SearchResponse),
    limit: u64,
) -> Result<T, String> {
    let mut line = Vec::new();
    for generation in generations {
        line.clear();
        reader
            .by_ref()
            .take(limit + 1)
            .read_until(SEARCH_LINE_END[0], &mut line)
            .map_err(|error| error.to_string())?;
        if line.len() as u64 > limit {
            return Err(String::from(WEATHER_RESPONSE_TOO_LARGE));
        }
        let response: SearchResponse =
            serde_json::from_slice(&line).map_err(|error| error.to_string())?;
        if response.generation != *generation {
            return Err(String::from(WEATHER_GENERATION_MISMATCH));
        }
        if let Some(error) = &response.error {
            return Err(error.clone());
        }
        consume(&mut result, response);
    }
    let mut extra = [0];
    if reader.read(&mut extra).map_err(|error| error.to_string())? != 0 {
        return Err(String::from(WEATHER_GENERATION_MISMATCH));
    }
    Ok(result)
}

fn clear_index(index: &Path) -> std::io::Result<()> {
    for path in [index.to_path_buf(), PathBuf::from(format!("{}.fingerprint", index.display()))] {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

async fn build_index(
    paths: &SemanticPaths,
    request: &BuildRequest,
    controls: &mut Receiver<TaskControl>,
    tasks: &TaskRegistry,
) -> anyhow::Result<()> {
    if let Some(parent) = paths.index.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let threads = std::thread::available_parallelism().map_or(1, usize::from).min(4);
    let started = Instant::now();
    log::info!("semantic index refresh: {} items with {threads} threads", request.entries.len());
    let mut command = crate::infrastructure::proc::tool_async(&paths.helper);
    command
        .kill_on_drop(true)
        .arg("--build-index")
        .arg("--manifest")
        .arg(&paths.manifest)
        .arg("--index")
        .arg(&paths.index)
        .arg("--runtime")
        .arg(&paths.runtime)
        .arg("--threads")
        .arg(threads.to_string())
        .arg("--progress-json")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    let mut child = crate::infrastructure::proc::spawn(&mut command)?;
    let pid = child.id().ok_or_else(|| anyhow::anyhow!("semantic indexer pid unavailable"))?;
    let mut stdin = child.stdin.take().ok_or_else(|| anyhow::anyhow!("semantic index stdin"))?;
    stdin.write_all(&serde_json::to_vec(request)?).await?;
    stdin.shutdown().await?;
    drop(stdin);
    let stdout = child.stdout.take().ok_or_else(|| anyhow::anyhow!("semantic index stdout"))?;
    let mut lines = tokio::io::BufReader::new(stdout).lines();
    let catalog_total = request.entries.len() as u64;
    let mut total = 0_u64;
    let mut progress = 0_u64;
    let mut detail = String::from("Checking cached embeddings");
    let mut state = TaskState::Running;
    let status = loop {
        tokio::select! {
            line = lines.next_line() => match line? {
                Some(line) => {
                    if let Ok(update) = serde_json::from_str::<BuildProgress>(&line) {
                        (progress, total) = normalized_progress(&update, catalog_total);
                        detail = update.detail;
                        update_index_task(tasks, state, progress, total, detail.clone());
                    }
                }
                None => break child.wait().await?,
            },
            Some(action) = controls.recv() => match action {
                TaskControl::Pause => {
                    let result = unsafe { libc::kill(pid as i32, libc::SIGSTOP) };
                    if result == 0 {
                        state = TaskState::Paused;
                        update_index_task(tasks, state, progress, total, String::from("Paused"));
                    }
                }
                TaskControl::Resume => {
                    let result = unsafe { libc::kill(pid as i32, libc::SIGCONT) };
                    if result == 0 {
                        state = TaskState::Running;
                        update_index_task(tasks, state, progress, total, detail.clone());
                    }
                }
                TaskControl::Stop => {
                    let _ = child.kill().await;
                    tasks.finish("semantic-index", TaskState::Cancelled, "Stopped");
                    anyhow::bail!("semantic index cancelled");
                }
            },
        }
    };
    anyhow::ensure!(status.success(), "semantic indexer exited with {status}");
    log::info!(
        "semantic index ready: {} items in {:.1}s",
        request.entries.len(),
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

fn normalized_progress(update: &BuildProgress, catalog_total: u64) -> (u64, u64) {
    let total = (update.total as u64).min(catalog_total);
    let progress = (update.progress as u64).min(total);
    (progress, total)
}

fn update_index_task(
    tasks: &TaskRegistry,
    state: TaskState,
    progress: u64,
    total: u64,
    detail: String,
) {
    let mut task = TaskStatus::running("semantic-index", "index", "Updating search index");
    task.state = state;
    task.progress = progress.min(total);
    task.total = total;
    task.detail = detail;
    task.capabilities = match state {
        TaskState::Running => TaskCapabilities { pause: true, stop: true, ..Default::default() },
        TaskState::Paused => TaskCapabilities { resume: true, stop: true, ..Default::default() },
        _ => TaskCapabilities::default(),
    };
    tasks.update(task);
}

fn catalog_request(database: &Database, multiview: bool) -> anyhow::Result<BuildRequest> {
    let rows = database
        .with_connection(|connection| skwd_wall_core::db::list_wallpapers(connection, false))?;
    Ok(request_from_rows(&rows, multiview))
}

fn request_from_rows(rows: &[serde_json::Value], multiview: bool) -> BuildRequest {
    let mut catalog_hasher = std::collections::hash_map::DefaultHasher::new();
    let mut entries = Vec::with_capacity(if multiview { rows.len() * 2 } else { rows.len() });
    for row in rows {
        if row.get("type").and_then(serde_json::Value::as_str) == Some(wall_proto::kind::SHADER) {
            continue;
        }
        let name = row.get("name").and_then(serde_json::Value::as_str).unwrap_or("");
        let name = name.trim_end_matches('/');
        if name.is_empty() {
            continue;
        }
        let key = row
            .get("key")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or(name);
        let thumb =
            row.get("thumb").and_then(serde_json::Value::as_str).filter(|value| !value.is_empty());
        let Some(thumb) = thumb else { continue };
        let mtime = row.get("mtime").and_then(serde_json::Value::as_i64).unwrap_or(0);
        key.hash(&mut catalog_hasher);
        thumb.hash(&mut catalog_hasher);
        mtime.hash(&mut catalog_hasher);
        entry_fingerprint(thumb, mtime, ImageView::Full).hash(&mut catalog_hasher);
        entries.push(BuildEntry {
            key: key.to_string(),
            path: PathBuf::from(thumb),
            fingerprint: entry_fingerprint(thumb, mtime, ImageView::Full),
            view: ImageView::Full,
        });
        if !multiview {
            continue;
        }
        "multiview-v1".hash(&mut catalog_hasher);
        let width = row.get("width").and_then(serde_json::Value::as_i64).unwrap_or(0);
        let height = row.get("height").and_then(serde_json::Value::as_i64).unwrap_or(0);
        width.hash(&mut catalog_hasher);
        height.hash(&mut catalog_hasher);
        entries.push(BuildEntry {
            key: key.to_string(),
            path: PathBuf::from(thumb),
            fingerprint: entry_fingerprint(thumb, mtime, ImageView::Center),
            view: ImageView::Center,
        });
        if row.get("type").and_then(serde_json::Value::as_str) == Some(wall_proto::kind::STATIC)
            && width > 0
            && height > 0
            && width >= height.saturating_mul(2)
        {
            for view in [ImageView::LeftThird, ImageView::RightThird] {
                entries.push(BuildEntry {
                    key: key.to_string(),
                    path: PathBuf::from(thumb),
                    fingerprint: entry_fingerprint(thumb, mtime, view),
                    view,
                });
            }
        }
    }
    BuildRequest { fingerprint: catalog_hasher.finish(), entries }
}

fn entry_fingerprint(thumb: &str, mtime: i64, view: ImageView) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    thumb.hash(&mut hasher);
    mtime.hash(&mut hasher);
    if let Ok(metadata) = std::fs::metadata(thumb) {
        metadata.len().hash(&mut hasher);
        metadata.modified().ok().hash(&mut hasher);
    }
    match view {
        ImageView::Full => {}
        ImageView::Center => "center".hash(&mut hasher),
        ImageView::LeftThird => "leftThird".hash(&mut hasher),
        ImageView::RightThird => "rightThird".hash(&mut hasher),
    }
    hasher.finish()
}

fn discover_paths(config: &ConfigStore) -> Option<SemanticPaths> {
    let executable = std::env::current_exe().ok()?;
    let bin_dir = executable.parent()?;
    let configured_root = env_path_with_fallback("SKWD_LENS_HOME", "SKWD_SEMANTIC_HOME");
    let roots = configured_root
        .map_or_else(|| default_semantic_roots(bin_dir), |configured| vec![configured]);
    let search_path = std::env::var_os("PATH");
    let helper = resolve_lens_helper(
        bin_dir,
        env_path_with_fallback("SKWD_LENS_BIN", "SKWD_SEMANTIC_BIN"),
        search_path.as_deref(),
    )?;
    let configured = config.read();
    let selected_manifest = configured.semantic_manifest();
    let profile = match configured.semantic_index_profile().as_str() {
        "multiview" => "multiview",
        _ => "full",
    };
    drop(configured);
    let configured_manifest =
        env_path_with_fallback("SKWD_LENS_MANIFEST", "SKWD_SEMANTIC_MANIFEST").or_else(|| {
            (!selected_manifest.trim().is_empty()).then(|| PathBuf::from(selected_manifest))
        });
    let custom_manifest = configured_manifest.is_some();
    let configured_runtime =
        env_path_with_fallback("SKWD_LENS_ORT_DYLIB", "SKWD_SEMANTIC_ORT_DYLIB");
    let (_, manifest, runtime) = resolve_semantic_assets(
        &roots,
        configured_manifest.as_deref(),
        configured_runtime.as_deref(),
    )?;
    let cache = PathBuf::from(config.read().cache_dir());
    let index = env_path_with_fallback("SKWD_LENS_INDEX", "SKWD_SEMANTIC_INDEX").or_else(|| {
        if !custom_manifest && profile == "full" {
            return Some(cache.join("semantic/index-siglip2.sidx"));
        }
        let identity = manifest_identity(&manifest).ok()?;
        Some(cache.join("semantic").join(cache_index_name(&identity, profile)))
    })?;
    Some(SemanticPaths { helper, manifest, runtime, index, multiview: profile == "multiview" })
}

fn env_path_with_fallback(canonical: &str, legacy: &str) -> Option<PathBuf> {
    std::env::var_os(canonical).or_else(|| std::env::var_os(legacy)).map(PathBuf::from)
}

fn default_semantic_roots(bin_dir: &Path) -> Vec<PathBuf> {
    let mut roots = vec![
        skwd_wall_core::paths::lens_data_dir().join("models").join("semantic"),
        bin_dir.join("lens"),
        skwd_wall_core::paths::data_dir().join("models").join("semantic"),
        bin_dir.join("semantic"),
    ];
    if let Some(prefix) = bin_dir.parent() {
        roots.push(prefix.join("share/skwd-lens/models/semantic"));
    }
    roots
}

fn resolve_semantic_assets(
    roots: &[PathBuf],
    configured_manifest: Option<&Path>,
    configured_runtime: Option<&Path>,
) -> Option<(PathBuf, PathBuf, PathBuf)> {
    if configured_manifest.is_some_and(|path| !path.is_file())
        || configured_runtime.is_some_and(|path| !path.is_file())
    {
        return None;
    }
    roots.iter().find_map(|root| {
        let manifest =
            configured_manifest.map_or_else(|| root.join("semantic-pack.json"), Path::to_path_buf);
        if !manifest.is_file() {
            return None;
        }
        let runtime = configured_runtime.map(Path::to_path_buf).or_else(|| find_runtime(root))?;
        Some((root.clone(), manifest, runtime))
    })
}

fn resolve_lens_helper(
    bin_dir: &Path,
    configured: Option<PathBuf>,
    search_path: Option<&OsStr>,
) -> Option<PathBuf> {
    if let Some(configured) = configured {
        if skwd_wall_core::paths::is_executable(&configured) {
            return Some(configured);
        }
        if configured.components().count() == 1 {
            return skwd_wall_core::paths::resolve_binary(
                Some(bin_dir),
                search_path,
                configured.as_os_str(),
            );
        }
        return None;
    }
    skwd_wall_core::paths::resolve_preferred_binary(
        Some(bin_dir),
        search_path,
        &["skwd-lens", "skwd-wall-semantic"],
    )
}

fn find_runtime(root: &Path) -> Option<PathBuf> {
    [
        root.join("runtime/libonnxruntime.so.1.27.0"),
        root.join("runtime/libonnxruntime.so"),
        root.join("runtime/libonnxruntime.dylib"),
        root.join("runtime/onnxruntime.dll"),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

fn index_current(paths: &SemanticPaths, fingerprint: u64) -> bool {
    let sidecar = PathBuf::from(format!("{}.fingerprint", paths.index.display()));
    let fingerprint_matches =
        std::fs::read_to_string(sidecar).ok().and_then(|value| value.parse::<u64>().ok())
            == Some(fingerprint);
    fingerprint_matches && expected_model(&paths.manifest) == read_index_model(&paths.index)
}

fn expected_model(manifest: &Path) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(manifest).ok()?).ok()?;
    Some(format!("{}@{}", value.get("id")?.as_str()?, value.get("version")?.as_str()?))
}

fn read_index_model(index: &Path) -> Option<String> {
    let mut reader = std::fs::File::open(index).ok()?;
    let mut magic = [0_u8; 8];
    reader.read_exact(&mut magic).ok()?;
    if &magic != b"SKWDSEM3" {
        return None;
    }
    let mut value = [0_u8; 4];
    reader.read_exact(&mut value).ok()?;
    reader.read_exact(&mut value).ok()?;
    let length = u32::from_le_bytes(value) as usize;
    if length == 0 || length > 4_096 {
        return None;
    }
    let mut model = vec![0_u8; length];
    reader.read_exact(&mut model).ok()?;
    String::from_utf8(model).ok()
}

#[cfg(test)]
mod tests;
