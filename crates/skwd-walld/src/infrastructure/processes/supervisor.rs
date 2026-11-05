use std::sync::Arc;

use skwd_wall_core::WallState;
use skwd_wall_core::infrastructure::config::ConfigStore;

use crate::backend::events::EventPublisher;
use crate::backend::workers::MediaWorkerSupervisor;
use crate::infrastructure::stats::Stats;
use crate::infrastructure::tasks::TaskRegistry;

use super::super::image_optimization::ImageOptimizer;
use super::scanner;

pub(crate) struct ProcessSupervisor {
    thumbnail_capture: Arc<super::thumbnails::ThumbnailBatch>,
    state: Arc<WallState>,
    debug: bool,
    image_optimizer: Arc<ImageOptimizer>,
    tasks: Arc<TaskRegistry>,
    stats: Arc<Stats>,
}

impl ProcessSupervisor {
    pub(crate) fn new(
        state: Arc<WallState>,
        config: Arc<ConfigStore>,
        publisher: Arc<dyn EventPublisher>,
        tasks: Arc<TaskRegistry>,
        stats: Arc<Stats>,
        debug: bool,
    ) -> Self {
        let image_optimizer = Arc::new(ImageOptimizer::new(
            Arc::clone(&state),
            config,
            publisher,
            Arc::new(std::sync::Mutex::new(())),
            debug,
        ));
        Self {
            thumbnail_capture: Arc::new(super::thumbnails::ThumbnailBatch::new(
                Arc::clone(&state),
                Arc::clone(&tasks),
            )),
            image_optimizer,
            state,
            debug,
            tasks,
            stats,
        }
    }
}

impl MediaWorkerSupervisor for ProcessSupervisor {
    fn capture_scene_thumbnails(&self) -> bool {
        self.thumbnail_capture.start()
    }
    fn stop_scene_thumbnails(&self) -> bool {
        self.thumbnail_capture.stop()
    }
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
}
