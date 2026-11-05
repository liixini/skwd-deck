use paper_control::{Background, WidgetScene};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Desk {
    pub scene: WidgetScene,
    pub outputs: Vec<DeskOutput>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeskOutput {
    pub name: String,
    pub path: String,
    pub fill: String,
    pub background: Background,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Editing {
    pub active: bool,
}

#[cfg(test)]
#[path = "widgets_tests.rs"]
mod tests;
