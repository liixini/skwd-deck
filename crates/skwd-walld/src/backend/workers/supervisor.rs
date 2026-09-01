pub(crate) struct TinierPreparation {
    pub(crate) task_id: String,
    pub(crate) result: std::sync::mpsc::Receiver<Result<(), String>>,
}

pub(crate) trait MediaWorkerSupervisor: Send + Sync {
    fn scan(&self, extra: &[&str], request_id: Option<&str>);
    fn remote_thumbnails(&self, source: &str, jobs: &[(String, String)]);
    fn optimize_images(&self, automatic: bool, changed_paths: &[String]) -> bool;
    fn image_optimization_status(&self) -> serde_json::Value;
    fn prepare_tinier(&self, source: &str) -> TinierPreparation;
    fn stop_tinier(&self, task_id: &str) -> bool;
}
