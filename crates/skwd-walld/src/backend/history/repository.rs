use crate::domain::history::{HistoryEntry, OutputHistory};

pub(crate) trait HistoryRepository: Send + Sync {
    fn record(
        &self,
        output: &str,
        entry: &HistoryEntry,
        prior: Option<&HistoryEntry>,
        depth: usize,
        live_outputs: &[String],
    );

    fn navigate(
        &self,
        output: &str,
        forward: bool,
        live_outputs: &[String],
    ) -> Vec<(String, HistoryEntry)>;

    fn list(&self, output: &str) -> Vec<(String, OutputHistory)>;
}
