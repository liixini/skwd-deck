use std::collections::{BTreeMap, HashMap, HashSet};
use std::process::{Child, ChildStdin};
use std::sync::Mutex;

use super::process_map::ChildMap;
use super::readiness::ReadinessRegistry;

#[derive(Default)]
pub struct RendererSupervisor {
    pub(super) fleet: Mutex<Vec<Child>>,
    pub(super) paper_child: Mutex<Option<Child>>,
    pub(super) paper_stdin: Mutex<Option<ChildStdin>>,
    pub(super) still_child: Mutex<Option<Child>>,
    pub(super) still_stdin: Mutex<Option<ChildStdin>>,
    pub(super) output_stills: Mutex<ChildMap>,
    pub(super) video_papers: Mutex<ChildMap>,
    pub(super) scene_papers: Mutex<std::collections::HashSet<String>>,
    pub(super) assignments: Mutex<HashMap<String, String>>,
    pub(super) policies: Mutex<HashMap<String, String>>,
    pub(super) pause: Mutex<PauseState>,
    pub(super) we_render: Mutex<WeRender>,
    pub(super) ready: ReadinessRegistry,
}

#[derive(Default)]
pub(super) struct PauseState {
    pub manual: bool,
    pub sessions: HashSet<u64>,
    pub applying: usize,
    pub session_rendering: HashMap<u32, u64>,
    pub next_session_rendering: u64,
}

impl PauseState {
    pub fn effective(&self) -> bool {
        self.applying == 0 && (self.manual || !self.sessions.is_empty())
    }

    pub fn policy(&self) -> PausePolicy {
        PausePolicy {
            manual: self.applying == 0 && self.manual,
            session: self.applying == 0 && !self.sessions.is_empty(),
        }
    }

    pub fn session_exempt(&self, pid: u32) -> bool {
        self.session_rendering.contains_key(&pid)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PausePolicy {
    pub manual: bool,
    pub session: bool,
}

impl PausePolicy {
    pub fn paused(self, session_exempt: bool) -> bool {
        self.manual || (self.session && !session_exempt)
    }
}

#[derive(Default, Clone, PartialEq)]
pub struct WeRender {
    pub groups: BTreeMap<String, Vec<String>>,
    pub audio: BTreeMap<String, (bool, u32)>,
}

pub type HeldRenderer = (Child, Option<ChildStdin>);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SceneFreezeHandle {
    pub(crate) key: String,
    pub(crate) pid: u32,
}

pub fn kill_held_renderer(held: HeldRenderer) {
    let (mut child, stdin) = held;
    super::process_map::kill_child(&mut child);
    drop(stdin);
}

#[cfg(test)]
pub(crate) fn capture_child(path: &std::path::Path) -> HeldRenderer {
    let mut child = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("exec cat > '{}'", path.display()))
        .stdin(std::process::Stdio::piped())
        .spawn()
        .expect("spawn capture child");
    let stdin = child.stdin.take();
    (child, stdin)
}

#[cfg(test)]
pub(crate) fn exited_child() -> HeldRenderer {
    let mut child = std::process::Command::new("sh")
        .arg("-c")
        .arg("exit 0")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .expect("spawn exiting child");
    let stdin = child.stdin.take();
    let _ = child.wait();
    (child, stdin)
}
