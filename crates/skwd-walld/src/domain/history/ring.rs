use serde::{Deserialize, Serialize};

use super::HistoryEntry;

#[derive(Clone, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub(crate) struct OutputHistory {
    pub entries: Vec<HistoryEntry>,
    pub pos: usize,
}

impl OutputHistory {
    pub(crate) fn normalize(&mut self) {
        if self.entries.is_empty() {
            self.pos = 0;
        } else if self.pos >= self.entries.len() {
            self.pos = self.entries.len() - 1;
        }
    }

    fn seed_if_empty(&mut self, prior: Option<&HistoryEntry>) {
        if !self.entries.is_empty() {
            return;
        }
        if let Some(prior) = prior.filter(|prior| prior.is_valid()) {
            self.entries.push(prior.clone());
            self.pos = 0;
        }
    }

    pub(crate) fn record(
        &mut self,
        entry: &HistoryEntry,
        prior: Option<&HistoryEntry>,
        depth: usize,
    ) {
        self.seed_if_empty(prior);
        if self.entries.get(self.pos) == Some(entry) {
            return;
        }
        if !self.entries.is_empty() {
            self.entries.truncate(self.pos + 1);
        }
        self.entries.push(entry.clone());
        self.pos = self.entries.len() - 1;
        self.cap(depth);
    }

    fn cap(&mut self, depth: usize) {
        let depth = depth.max(1);
        if self.entries.len() > depth {
            let remove = self.entries.len() - depth;
            self.entries.drain(0..remove);
            self.pos = self.pos.saturating_sub(remove);
        }
    }

    pub(crate) fn back(&mut self) -> Option<HistoryEntry> {
        if self.pos > 0 {
            self.pos -= 1;
            self.entries.get(self.pos).cloned()
        } else {
            None
        }
    }

    pub(crate) fn forward(&mut self) -> Option<HistoryEntry> {
        if self.pos + 1 < self.entries.len() {
            self.pos += 1;
            self.entries.get(self.pos).cloned()
        } else {
            None
        }
    }
}
