use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use wall_proto::{TaskState, TaskStatus, ev};

use crate::backend::events::EventPublisher;
use crate::infrastructure::events::EventHub;

pub(crate) struct TaskRegistry {
    tasks: Mutex<BTreeMap<String, TaskStatus>>,
    events: Arc<EventHub>,
}

impl TaskRegistry {
    pub(crate) fn new(events: Arc<EventHub>) -> Self {
        Self { tasks: Mutex::new(BTreeMap::new()), events }
    }

    pub(crate) fn update(&self, task: TaskStatus) {
        skwd_wall_core::lock(&self.tasks).insert(task.id.clone(), task.clone());
        if let Ok(value) = serde_json::to_value(task) {
            self.events.publish(ev::TASK_STATUS, value);
        }
    }

    pub(crate) fn list(&self) -> Vec<TaskStatus> {
        skwd_wall_core::lock(&self.tasks).values().cloned().collect()
    }

    pub(crate) fn finish(&self, id: &str, state: TaskState, detail: impl Into<String>) {
        self.finish_inner(id, state, detail.into(), false);
    }

    pub(crate) fn finish_if_active(
        &self,
        id: &str,
        state: TaskState,
        detail: impl Into<String>,
    ) -> bool {
        self.finish_inner(id, state, detail.into(), true)
    }

    fn finish_inner(&self, id: &str, state: TaskState, detail: String, active_only: bool) -> bool {
        let task = {
            let mut tasks = skwd_wall_core::lock(&self.tasks);
            let Some(task) = tasks.get_mut(id) else {
                return false;
            };
            if active_only && !matches!(task.state, TaskState::Running | TaskState::Paused) {
                return false;
            }
            task.state = state;
            if state == TaskState::Completed && task.total > 0 {
                task.progress = task.total;
            }
            task.detail = detail;
            task.capabilities = wall_proto::TaskCapabilities::default();
            task.clone()
        };
        if let Ok(value) = serde_json::to_value(task) {
            self.events.publish(ev::TASK_STATUS, value);
        }
        true
    }
}
