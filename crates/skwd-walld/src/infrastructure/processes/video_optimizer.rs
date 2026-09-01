use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use skwd_wall_core::infrastructure::config::ConfigStore;
use skwd_wall_core::infrastructure::database::Database;
use skwd_wall_core::lock;

use crate::backend::workers::TinierPreparation;
use crate::domain::video_optimization::fit_cap_height;
use crate::infrastructure::tasks::TaskRegistry;
use crate::infrastructure::video_optimization::{
    decode_probe_ok, probe, probe_duration_ms, probe_frame_rate, spawn_encoder, tinier_dest_path,
    tinier_encode_args,
};

const TINIER_MAX_BYTES: i64 = skwd_wall_core::db::TINIER_CONVERT_MAX_BYTES as i64;
const TINIER_MAX_HEIGHT: u32 = 1440;
const TINIER_MAX_FPS: u32 = 30;
const TINIER_PRESET: &str = skwd_wall_core::db::TINIER_CONVERT_PRESET;

struct TinierSubscribers {
    result: Option<Result<(), String>>,
    senders: Vec<Sender<Result<(), String>>>,
}

struct TinierWork {
    source: String,
    task_id: String,
    cancelled: AtomicBool,
    interactive: AtomicBool,
    subscribers: Mutex<TinierSubscribers>,
}

impl TinierWork {
    fn new(source: &str) -> Self {
        Self {
            source: source.to_string(),
            task_id: tinier_task_id(source),
            cancelled: AtomicBool::new(false),
            interactive: AtomicBool::new(false),
            subscribers: Mutex::new(TinierSubscribers { result: None, senders: Vec::new() }),
        }
    }

    fn subscribe(&self) -> std::sync::mpsc::Receiver<Result<(), String>> {
        self.interactive.store(true, Ordering::Release);
        let (sender, receiver) = std::sync::mpsc::channel();
        let mut subscribers = lock(&self.subscribers);
        if let Some(result) = subscribers.result.clone() {
            let _ = sender.send(result);
        } else {
            subscribers.senders.push(sender);
        }
        receiver
    }

    fn finish(&self, result: Result<(), String>) {
        let mut subscribers = lock(&self.subscribers);
        subscribers.result = Some(result);
        let senders = std::mem::take(&mut subscribers.senders);
        let result = subscribers.result.as_ref().expect("Tinier result was just stored");
        for sender in senders {
            let _ = sender.send(result.clone());
        }
    }
}

pub(super) struct VideoOptimizer {
    config: Arc<ConfigStore>,
    database: Arc<Database>,
    queue: Mutex<VecDeque<Arc<TinierWork>>>,
    running: AtomicBool,
    work_gate: Arc<Mutex<()>>,
    current_tinier: Mutex<Option<Arc<TinierWork>>>,
    tasks: Arc<TaskRegistry>,
}

impl VideoOptimizer {
    pub(super) fn new(
        config: Arc<ConfigStore>,
        database: Arc<Database>,
        work_gate: Arc<Mutex<()>>,
        tasks: Arc<TaskRegistry>,
    ) -> Self {
        Self {
            config,
            database,
            queue: Mutex::new(VecDeque::new()),
            running: AtomicBool::new(false),
            work_gate,
            current_tinier: Mutex::new(None),
            tasks,
        }
    }

    pub(super) fn prepare(self: &Arc<Self>, source: &str) -> TinierPreparation {
        let work = self.find_or_queue(source);
        let result = work.subscribe();
        self.publish_task(&work, 0, "Queued");
        self.spawn_next();
        TinierPreparation { task_id: work.task_id.clone(), result }
    }

    pub(super) fn stop(&self, task_id: &str) -> bool {
        if let Some(work) = lock(&self.current_tinier).as_ref()
            && work.task_id == task_id
        {
            work.cancelled.store(true, Ordering::Release);
            return true;
        }
        let removed = {
            let mut queue = lock(&self.queue);
            queue
                .iter()
                .position(|work| work.task_id == task_id)
                .and_then(|index| queue.remove(index))
        };
        let Some(work) = removed else {
            return false;
        };
        let detail = String::from("Preparation cancelled");
        work.finish(Err(detail.clone()));
        self.tasks.finish(task_id, wall_proto::TaskState::Cancelled, detail);
        true
    }

    fn find_or_queue(&self, source: &str) -> Arc<TinierWork> {
        if let Some(current) = lock(&self.current_tinier).as_ref()
            && current.source == source
        {
            return Arc::clone(current);
        }
        let mut queue = lock(&self.queue);
        if let Some(index) = queue.iter().position(|work| work.source == source)
            && let Some(work) = queue.remove(index)
        {
            if let Some(current) = lock(&self.current_tinier).as_ref() {
                current.cancelled.store(true, Ordering::Release);
            }
            queue.push_front(Arc::clone(&work));
            return work;
        }
        if let Some(current) = lock(&self.current_tinier).as_ref() {
            current.cancelled.store(true, Ordering::Release);
        }
        let work = Arc::new(TinierWork::new(source));
        queue.push_front(Arc::clone(&work));
        work
    }

    fn publish_task(&self, work: &TinierWork, progress: u64, state: &str) {
        if work.interactive.load(Ordering::Acquire) {
            self.tasks.update(tinier_task_status(work, progress, state));
        }
    }

    fn spawn_next(self: &Arc<Self>) {
        if self.running.swap(true, Ordering::AcqRel) {
            return;
        }
        let optimizer = Arc::clone(self);
        crate::infrastructure::proc::runtime().spawn(async move {
            loop {
                let Some(work) = optimizer.pop_task() else {
                    return;
                };
                let worker = Arc::clone(&optimizer);
                let _ = tokio::task::spawn_blocking(move || worker.run_task(&work)).await;
            }
        });
    }

    fn pop_task(&self) -> Option<Arc<TinierWork>> {
        let mut queue = lock(&self.queue);
        let next = queue.pop_front();
        if next.is_none() {
            self.running.store(false, Ordering::Release);
        }
        next
    }

    fn run_task(&self, work: &Arc<TinierWork>) {
        let _permit = lock(&self.work_gate);
        if self.config.read().renderer().video_engine() != "tinier" {
            let detail = String::from("Tinier is no longer selected");
            if work.interactive.load(Ordering::Acquire) {
                self.tasks.finish(&work.task_id, wall_proto::TaskState::Cancelled, &detail);
            }
            work.finish(Err(detail));
            return;
        }
        *lock(&self.current_tinier) = Some(Arc::clone(work));
        self.publish_task(work, 0, "Starting");
        let result = convert_tinier(&self.config, &self.database, work, &self.tasks);
        let (state, detail) = match &result {
            Ok(()) => (wall_proto::TaskState::Completed, "Prepared".to_string()),
            Err(error) if work.cancelled.load(Ordering::Acquire) => {
                (wall_proto::TaskState::Cancelled, error.clone())
            }
            Err(error) => (wall_proto::TaskState::Failed, error.clone()),
        };
        if work.interactive.load(Ordering::Acquire) {
            self.tasks.finish(&work.task_id, state, detail);
        }
        work.finish(result);
        let mut current = lock(&self.current_tinier);
        if current.as_ref().is_some_and(|active| Arc::ptr_eq(active, work)) {
            *current = None;
        }
    }
}

fn convert_tinier(
    config: &ConfigStore,
    database: &Database,
    work: &TinierWork,
    tasks: &TaskRegistry,
) -> Result<(), String> {
    let source = work.source.as_str();
    let Some((_, width, height, fps)) = probe(source) else {
        return Err(format!("Could not inspect {}", display_name(source)));
    };
    let outputs: Vec<(u32, u32)> = skwd_wall_core::outputs::enumerate()
        .into_iter()
        .map(|monitor| {
            let (width, height) = monitor.logical_size();
            (width.max(0) as u32, height.max(0) as u32)
        })
        .collect();
    let fit_height = fit_cap_height(width, height, &outputs);
    let max_height =
        if fit_height == 0 { TINIER_MAX_HEIGHT } else { fit_height.min(TINIER_MAX_HEIGHT) };
    let fps_cap = (fps > f64::from(TINIER_MAX_FPS)).then_some(TINIER_MAX_FPS);
    let destination = {
        let cache = config.read().cache_dir();
        tinier_dest_path(std::path::Path::new(&cache), source)
    };
    if let Some(directory) = destination.parent()
        && let Err(error) = std::fs::create_dir_all(directory)
    {
        return Err(format!("Cannot create Tinier cache: {error}"));
    }
    let destination_text = destination.to_string_lossy().into_owned();
    let arguments = tinier_encode_args(source, &destination_text, max_height, fps_cap);
    log::info!("tinier optimize: converting {source} -> {destination_text}");
    let started = std::time::Instant::now();
    let result = run_tinier_encoder(&arguments, source, work, tasks, &destination)?;
    let size = std::fs::metadata(&destination).map_or(0, |metadata| metadata.len() as i64);
    let decode_ok = result.status.success()
        && size > 0
        && size <= TINIER_MAX_BYTES
        && decode_probe_ok(&destination_text, "av1");
    let frame_rate = decode_ok.then(|| probe_frame_rate(&destination_text)).flatten();
    let Some(frame_rate) = frame_rate else {
        let detail = if !result.status.success() {
            stderr_tail(&result.stderr)
        } else if size == 0 {
            String::from("encoder produced an empty file")
        } else if size > TINIER_MAX_BYTES {
            String::from("encoded file exceeded the 256 MiB limit")
        } else if !decode_ok {
            String::from("dav1d could not decode the encoded file")
        } else {
            String::from("encoded file has no usable frame rate")
        };
        log::warn!("tinier optimize: invalid output for {source}: {detail}");
        let _ = std::fs::remove_file(destination);
        return Err(format!("AV1 validation failed for {}: {detail}", display_name(source)));
    };
    let original_size = std::fs::metadata(source).map_or(0, |metadata| metadata.len() as i64);
    if let Err(error) = database.with_connection(|connection| {
        skwd_wall_core::db::tinier_convert_record(
            connection,
            source,
            &destination_text,
            &frame_rate,
            TINIER_PRESET,
            original_size,
            size,
        )
    }) {
        log::warn!("tinier optimize: cannot record {source}: {error}");
        let _ = std::fs::remove_file(destination);
        return Err(format!("Cannot record Tinier preparation: {error}"));
    }
    log::info!(
        "tinier optimize: {source} prepared at {frame_rate} in {}s ({} MB resident source)",
        started.elapsed().as_secs(),
        size / 1_048_576
    );
    Ok(())
}

fn run_tinier_encoder(
    arguments: &[String],
    source: &str,
    work: &TinierWork,
    tasks: &TaskRegistry,
    destination: &Path,
) -> Result<std::process::Output, String> {
    let duration = probe_duration_ms(source);
    let mut child =
        spawn_encoder(arguments).map_err(|error| format!("Cannot start ffmpeg: {error}"))?;
    let stdout = child.stdout.take().ok_or_else(|| String::from("ffmpeg progress pipe missing"))?;
    let mut reported = 0;
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if work.cancelled.load(Ordering::Acquire) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(destination);
            return Err(String::from("Preparation cancelled"));
        }
        let Some(progress) = tinier_progress(&line, duration) else {
            continue;
        };
        if progress > reported {
            reported = progress;
            if work.interactive.load(Ordering::Acquire) {
                tasks.update(tinier_task_status(work, progress, "Encoding"));
            }
        }
    }
    if work.cancelled.load(Ordering::Acquire) {
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_file(destination);
        return Err(String::from("Preparation cancelled"));
    }
    child.wait_with_output().map_err(|error| format!("ffmpeg failed: {error}"))
}

fn tinier_progress(line: &str, duration_ms: Option<u64>) -> Option<u64> {
    if line == "progress=end" {
        return Some(99);
    }
    let elapsed_us = line.strip_prefix("out_time_us=")?.parse::<u64>().ok()?;
    let duration_us = duration_ms?.saturating_mul(1000);
    (duration_us > 0).then_some(elapsed_us.saturating_mul(100).checked_div(duration_us)?.min(99))
}

fn tinier_task_status(work: &TinierWork, progress: u64, state: &str) -> wall_proto::TaskStatus {
    let mut task = wall_proto::TaskStatus::running(
        work.task_id.clone(),
        "tinier-preparation",
        "Preparing Tinier AV1",
    );
    task.progress = progress.min(100);
    task.total = 100;
    task.detail = format!("{state}: {}", display_name(&work.source));
    task.capabilities.stop = true;
    task
}

fn display_name(source: &str) -> String {
    Path::new(source)
        .file_name()
        .map_or_else(|| source.to_string(), |name| name.to_string_lossy().into_owned())
}

fn stderr_tail(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    text.trim().chars().rev().take(240).collect::<String>().chars().rev().collect()
}

fn tinier_task_id(source: &str) -> String {
    let hash = source.as_bytes().iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    format!("tinier:{hash:016x}")
}

#[cfg(test)]
#[path = "video_optimizer/tests.rs"]
mod tests;
