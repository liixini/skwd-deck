use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    #[default]
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskCapabilities {
    #[serde(default)]
    pub pause: bool,
    #[serde(default)]
    pub resume: bool,
    #[serde(default)]
    pub stop: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStatus {
    pub id: String,
    pub kind: String,
    pub label: String,
    #[serde(default)]
    pub state: TaskState,
    #[serde(default)]
    pub progress: u64,
    #[serde(default)]
    pub total: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub eta: String,
    #[serde(default)]
    pub capabilities: TaskCapabilities,
}

impl TaskStatus {
    pub fn running(
        id: impl Into<String>,
        kind: impl Into<String>,
        label: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            kind: kind.into(),
            label: label.into(),
            state: TaskState::Running,
            progress: 0,
            total: 0,
            detail: String::new(),
            eta: String::new(),
            capabilities: TaskCapabilities::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskControl {
    Pause,
    Resume,
    Stop,
}
