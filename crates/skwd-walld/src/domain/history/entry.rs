use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub(crate) struct HistoryEntry {
    pub ty: String,
    pub path: String,
    pub we_id: String,
    pub mute: bool,
    pub volume: u32,
}

impl HistoryEntry {
    pub(crate) fn new(ty: &str, path: &str, we_id: &str, mute: bool, volume: u32) -> Self {
        Self { ty: ty.to_string(), path: path.to_string(), we_id: we_id.to_string(), mute, volume }
    }

    pub(crate) fn is_valid(&self) -> bool {
        if self.ty.is_empty() {
            return false;
        }
        if self.ty == "we" { !self.we_id.is_empty() } else { !self.path.is_empty() }
    }
}
