use std::sync::Arc;

use skwd_wall_core::WallState;
use skwd_wall_core::infrastructure::config::ConfigStore;
use skwd_wall_core::infrastructure::database::Database;

use crate::backend::events::EventPublisher;
use crate::backend::workers::{MediaWorkerSupervisor, TinierPreparation};
use crate::infrastructure::stats::Stats;
use crate::infrastructure::tasks::TaskRegistry;

use super::super::image_optimization::ImageOptimizer;
use super::scanner;
use super::video_optimizer::VideoOptimizer;

pub(crate) struct ProcessSupervisor {
    state: Arc<WallState>,
    debug: bool,
    image_optimizer: Arc<ImageOptimizer>,
    video_optimizer: Arc<VideoOptimizer>,
    tasks: Arc<TaskRegistry>,
    stats: Arc<Stats>,
}

impl ProcessSupervisor {
    pub(crate) fn new(
        state: Arc<WallState>,
        config: Arc<ConfigStore>,
        database: Arc<Database>,
        publisher: Arc<dyn EventPublisher>,
        tasks: Arc<TaskRegistry>,
        stats: Arc<Stats>,
        debug: bool,
    ) -> Self {
        let work_gate = Arc::new(std::sync::Mutex::new(()));
        let image_optimizer = Arc::new(ImageOptimizer::new(
            Arc::clone(&state),
            Arc::clone(&config),
            publisher,
            Arc::clone(&work_gate),
            debug,
        ));
        Self {
            image_optimizer,
            video_optimizer: Arc::new(VideoOptimizer::new(
                config,
                database,
                work_gate,
                Arc::clone(&tasks),
            )),
            state,
            debug,
            tasks,
            stats,
        }
    }
}

impl MediaWorkerSupervisor for ProcessSupervisor {
    fn scan(&self, extra: &[&str], request_id: Option<&str>) {
        scanner::spawn_scan(
            &self.state,
            self.debug,
            extra,
            request_id,
            Some((Arc::clone(&self.tasks), Arc::clone(&self.stats))),
        );
    }

    fn remote_thumbnails(&self, source: &str, jobs: &[(String, String)]) {
        scanner::spawn_remote_thumbnails(source, jobs);
    }

    fn optimize_images(&self, automatic: bool, changed_paths: &[String]) -> bool {
        self.image_optimizer.kick(automatic, changed_paths)
    }

    fn image_optimization_status(&self) -> serde_json::Value {
        self.image_optimizer.status().to_value()
    }

    fn prepare_tinier(&self, source: &str) -> TinierPreparation {
        self.video_optimizer.prepare(source)
    }

    fn stop_tinier(&self, task_id: &str) -> bool {
        self.video_optimizer.stop(task_id)
    }
}
