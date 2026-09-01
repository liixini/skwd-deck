use skwd_wall_core::lock;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::backend::history::HistoryRepository;
use crate::domain::history::{HistoryEntry, OutputHistory};

type Rings = HashMap<String, OutputHistory>;

pub(crate) struct FileHistoryRepository {
    path: PathBuf,
    io: Mutex<()>,
}

impl FileHistoryRepository {
    pub(crate) fn new(cache_dir: impl AsRef<Path>) -> Self {
        Self { path: cache_dir.as_ref().join("history.json"), io: Mutex::new(()) }
    }

    fn load(&self) -> Rings {
        let mut histories: Rings = std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        for history in histories.values_mut() {
            history.normalize();
        }
        histories
    }

    fn save(&self, histories: &Rings) {
        if let Some(directory) = self.path.parent() {
            let _ = std::fs::create_dir_all(directory);
        }
        let Ok(text) = serde_json::to_string(histories) else {
            return;
        };
        let _ = skwd_wall_core::paths::atomic_write(&self.path, text.as_bytes());
    }
}

impl HistoryRepository for FileHistoryRepository {
    fn record(
        &self,
        output: &str,
        entry: &HistoryEntry,
        prior: Option<&HistoryEntry>,
        depth: usize,
        live_outputs: &[String],
    ) {
        let _guard = lock(&self.io);
        let mut histories = self.load();
        for output in record_targets(output, live_outputs) {
            histories.entry(output).or_default().record(entry, prior, depth);
        }
        self.save(&histories);
    }

    fn navigate(
        &self,
        output: &str,
        forward: bool,
        live_outputs: &[String],
    ) -> Vec<(String, HistoryEntry)> {
        let _guard = lock(&self.io);
        let mut histories = self.load();
        let mut moved = Vec::new();
        for output in navigation_targets(output, live_outputs, &histories) {
            if let Some(history) = histories.get_mut(&output) {
                let entry = if forward { history.forward() } else { history.back() };
                if let Some(entry) = entry {
                    moved.push((output, entry));
                }
            }
        }
        if !moved.is_empty() {
            self.save(&histories);
        }
        moved
    }

    fn list(&self, output: &str) -> Vec<(String, OutputHistory)> {
        let histories = self.load();
        if output == "*" {
            histories.into_iter().collect()
        } else {
            let history = histories.get(output).cloned().unwrap_or_default();
            vec![(output.to_string(), history)]
        }
    }
}

fn record_targets(output: &str, live_outputs: &[String]) -> Vec<String> {
    if output != "*" {
        return vec![output.to_string()];
    }
    let outputs: Vec<String> =
        live_outputs.iter().filter(|output| output.as_str() != "*").cloned().collect();
    if outputs.is_empty() { vec!["*".to_string()] } else { outputs }
}

fn navigation_targets(output: &str, live_outputs: &[String], histories: &Rings) -> Vec<String> {
    if output != "*" {
        return vec![output.to_string()];
    }
    let outputs: Vec<String> = live_outputs
        .iter()
        .filter(|output| output.as_str() != "*" && histories.contains_key(*output))
        .cloned()
        .collect();
    if outputs.is_empty() && histories.contains_key("*") { vec!["*".to_string()] } else { outputs }
}
