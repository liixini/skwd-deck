use std::sync::Arc;

use skwd_wall_core::WallState;
use skwd_wall_core::backend::renderers::RendererSupervision;
use skwd_wall_core::backend::wallpaper::WallpaperApplication;
use skwd_wall_core::infrastructure::config::ConfigStore;
use skwd_wall_core::infrastructure::database::Database;

use crate::backend::history::HistoryRepository;
use crate::backend::workers::MediaWorkerSupervisor;
use crate::infrastructure::events::EventHub;
use crate::infrastructure::stats::Stats;
use crate::infrastructure::tasks::TaskRegistry;

#[derive(Clone)]
pub(crate) struct Ctx {
    pub state: Arc<WallState>,
    pub config: Arc<ConfigStore>,
    pub database: Arc<Database>,
    pub renderers: Arc<dyn RendererSupervision>,
    pub wallpaper: Arc<dyn WallpaperApplication>,
    pub history: Arc<dyn HistoryRepository>,
    pub events: Arc<EventHub>,
    pub workers: Arc<dyn MediaWorkerSupervisor>,
    pub stats: Arc<Stats>,
    pub tasks: Arc<TaskRegistry>,
}
