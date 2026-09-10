use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use skwd_wall_core::{WallState, db, lock, paths, we::ThumbnailCapture};
use wall_proto::{TaskState, TaskStatus, ThumbnailEncodeRequest, ThumbnailEncodeResponse};

use super::thumbnail_worker::{Children, Worker};
use crate::infrastructure::tasks::TaskRegistry;

const TASK_ID: &str = "we-thumbnails";

#[derive(Default)]
struct Control {
    cancelled: AtomicBool,
    children: Children,
}

pub(super) struct ThumbnailBatch {
    state: Arc<WallState>,
    tasks: Arc<TaskRegistry>,
    active: Mutex<Option<Arc<Control>>>,
}

impl ThumbnailBatch {
    pub(super) fn new(state: Arc<WallState>, tasks: Arc<TaskRegistry>) -> Self {
        Self { state, tasks, active: Mutex::new(None) }
    }

    pub(super) fn start(self: &Arc<Self>) -> bool {
        let mut active = lock(&self.active);
        if active.is_some() {
            return false;
        }
        let control = Arc::new(Control::default());
        *active = Some(Arc::clone(&control));
        self.progress(0, 0, "Preparing scene thumbnails".into());
        let batch = Arc::clone(self);
        std::thread::spawn(move || {
            if let Err(error) = batch.run(&control) {
                let state = if control.cancelled.load(Ordering::Acquire) {
                    TaskState::Cancelled
                } else {
                    TaskState::Failed
                };
                batch.tasks.finish(TASK_ID, state, error.to_string());
            }
            *lock(&batch.active) = None;
        });
        true
    }

    pub(super) fn stop(&self) -> bool {
        let active = lock(&self.active);
        let Some(control) = active.as_ref() else { return false };
        control.cancelled.store(true, Ordering::Release);
        for child in lock(&control.children).iter().filter_map(std::sync::Weak::upgrade) {
            let _ = lock(&child).kill();
        }
        true
    }

    fn progress(&self, progress: u64, total: u64, detail: String) {
        let mut task = TaskStatus::running(TASK_ID, "thumbnails", "Wallpaper Engine thumbnails");
        task.progress = progress;
        task.total = total;
        task.detail = detail;
        task.capabilities.stop = true;
        self.tasks.update(task);
    }

    fn run(&self, control: &Control) -> anyhow::Result<()> {
        let mut ids: Vec<String> = self
            .state
            .with_db(db::known_we_meta)?
            .into_iter()
            .filter(|(_, _, kind, _)| kind == "we")
            .filter_map(|(key, _, _, _)| key.strip_prefix("we:").map(str::to_owned))
            .collect();
        ids.sort();
        let total = ids.len() as u64;
        let directory = paths::cache_dir().join("scene-thumbnails");
        std::fs::create_dir_all(&directory)?;
        let next = AtomicUsize::new(0);
        let counts = Mutex::new((0, 0, 0));
        let workers = self.state.config().max_thumb_jobs().clamp(1, 2).min(ids.len());
        std::thread::scope(|scope| -> anyhow::Result<()> {
            let mut threads = Vec::new();
            for _ in 0..workers {
                threads
                    .push(scope.spawn(|| self.run_lane(control, &ids, &next, &counts, &directory)));
            }
            for thread in threads {
                thread.join().map_err(|_| anyhow::anyhow!("thumbnail worker panicked"))??;
            }
            Ok(())
        })?;
        let (captured, cached, failed) = *lock(&counts);
        let cancelled = control.cancelled.load(Ordering::Acquire);
        let state = if cancelled {
            TaskState::Cancelled
        } else if failed > 0 {
            TaskState::Failed
        } else {
            TaskState::Completed
        };
        self.progress(captured + cached + failed, total, String::new());
        self.tasks.finish(
            TASK_ID,
            state,
            format!("{captured} captured · {cached} cached · {failed} failed"),
        );
        Ok(())
    }

    fn run_lane(
        &self,
        control: &Control,
        ids: &[String],
        next: &AtomicUsize,
        counts: &Mutex<(u64, u64, u64)>,
        directory: &std::path::Path,
    ) -> anyhow::Result<()> {
        let temporary = tempfile::tempdir_in(directory)?;
        let frame = temporary.path().join("frame.png");
        let mut renderer = None;
        let mut encoder = None;
        let mut rendered = 0;
        while !control.cancelled.load(Ordering::Acquire) {
            let Some(id) = ids.get(next.fetch_add(1, Ordering::Relaxed)) else { break };
            {
                let counts = lock(counts);
                let (captured, cached, failed) = *counts;
                self.progress(
                    captured + cached + failed,
                    ids.len() as u64,
                    format!("{id} · {captured} captured · {cached} cached · {failed} failed"),
                );
            }
            let result = self.capture(control, id, &frame, &mut renderer, &mut encoder);
            if control.cancelled.load(Ordering::Acquire) {
                break;
            }
            match result {
                Ok(true) => {
                    lock(counts).0 += 1;
                    rendered += 1;
                    if rendered == 8 {
                        renderer = None;
                        rendered = 0;
                    }
                }
                Ok(false) => lock(counts).1 += 1,
                Err(error) => {
                    lock(counts).2 += 1;
                    log::warn!("scene thumbnail {id}: {error:#}");
                    renderer = None;
                    encoder = None;
                    rendered = 0;
                }
            }
        }
        Ok(())
    }

    fn capture(
        &self,
        control: &Control,
        id: &str,
        frame: &std::path::Path,
        renderer: &mut Option<Worker>,
        encoder: &mut Option<Worker>,
    ) -> anyhow::Result<bool> {
        let deadline = Instant::now() + Duration::from_secs(30);
        let capture = loop {
            anyhow::ensure!(
                !control.cancelled.load(Ordering::Acquire),
                "Thumbnail capture stopped"
            );
            if let Some(capture) = ThumbnailCapture::try_open(&self.state, id)? {
                break capture;
            }
            anyhow::ensure!(Instant::now() < deadline, "thumbnail is busy");
            std::thread::sleep(Duration::from_millis(25));
        };
        if capture.cached(&self.state)? {
            return Ok(false);
        }
        if renderer.is_none() {
            let config = self.state.config();
            let mut command = crate::infrastructure::proc::tool(config.renderer().paper_bin());
            command.arg("capture-scenes").env("SKWD_VK_DEVICE", config.renderer().gpu_device());
            *renderer = Some(Worker::spawn(command, &control.children)?);
        }
        anyhow::ensure!(!control.cancelled.load(Ordering::Acquire), "Thumbnail capture stopped");
        let response: paper_control::SceneThumbnailResponse =
            renderer.as_mut().unwrap().exchange(&paper_control::SceneThumbnailRequest {
                source: capture.source().to_string_lossy().into_owned(),
                destination: frame.to_string_lossy().into_owned(),
                properties: capture.properties().clone(),
            })?;
        anyhow::ensure!(
            response.source == capture.source().to_string_lossy(),
            "thumbnail source mismatch"
        );
        if let Some(error) = response.error {
            anyhow::bail!(error);
        }
        anyhow::ensure!(!control.cancelled.load(Ordering::Acquire), "Thumbnail capture stopped");
        anyhow::ensure!(capture.unchanged(&self.state)?, "scene changed during thumbnail capture");
        if encoder.is_none() {
            let mut command =
                crate::infrastructure::proc::tool(paths::sibling_bin("skwd-wall-scan"));
            command.arg("--scene-thumbnail-stream");
            super::scanner::apply_scan_limits(&mut command, self.state.config().max_thumb_jobs());
            *encoder = Some(Worker::spawn(command, &control.children)?);
        }
        anyhow::ensure!(!control.cancelled.load(Ordering::Acquire), "Thumbnail capture stopped");
        let response: ThumbnailEncodeResponse =
            encoder.as_mut().unwrap().exchange(&ThumbnailEncodeRequest {
                we_id: id.into(),
                image: frame.to_string_lossy().into_owned(),
            })?;
        if let Some(error) = response.error {
            anyhow::bail!(error);
        }
        capture.record(&self.state)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_unblocks_a_busy_scene_and_releases_the_batch_slot() {
        let (_guard, root) = crate::testenv::lock();
        crate::testenv::write_config(serde_json::json!({}));
        let (state, events, _) = crate::testenv::harness();
        state
            .with_db(|conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO meta (key, type) VALUES ('we:112233', 'we')",
                    [],
                )
            })
            .unwrap();
        let directory = paths::cache_dir().join("scene-thumbnails");
        std::fs::create_dir_all(&directory).unwrap();
        let guard = std::fs::File::create(directory.join("112233.lock")).unwrap();
        guard.lock().unwrap();
        let tasks = Arc::new(TaskRegistry::new(events));
        let batch = Arc::new(ThumbnailBatch::new(state, tasks.clone()));
        assert!(batch.start());
        assert!(!batch.start());
        let deadline = Instant::now() + Duration::from_secs(2);
        while tasks.list()[0].total == 0 {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(batch.stop());
        while lock(&batch.active).is_some() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let task = tasks.list().remove(0);
        assert_eq!(task.state, TaskState::Cancelled);
        assert!(!task.capabilities.stop);
        assert!(!batch.stop());
        assert!(!root.join("we/112233").exists());
    }
}
