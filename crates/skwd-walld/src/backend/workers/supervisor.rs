pub(crate) trait MediaWorkerSupervisor: Send + Sync {
    fn capture_scene_thumbnails(&self) -> bool;
    fn stop_scene_thumbnails(&self) -> bool;
    fn scan(&self, extra: &[&str], request_id: Option<&str>);
    fn remote_thumbnails(&self, source: &str, jobs: &[(String, String)]);
    fn optimize_images(&self, automatic: bool, changed_paths: &[String]) -> bool;
    fn image_optimization_status(&self) -> serde_json::Value;
}
