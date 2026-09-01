pub(crate) trait EventPublisher: Send + Sync {
    fn publish(&self, event: &str, data: serde_json::Value);
}
