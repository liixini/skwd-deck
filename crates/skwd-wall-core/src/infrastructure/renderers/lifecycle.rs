use std::collections::{BTreeMap, HashMap};
use std::process::{Child, ChildStdin};
use std::time::Duration;

use crate::lock;

use super::process_map::{
    kill_all, kill_child, kill_one, map_all_alive, map_entry_alive, map_entry_pid_alive, reap,
    replace, retain, slot_alive, slot_pid_alive,
};
use super::supervisor::{HeldRenderer, RendererSupervisor, WeRender};

impl RendererSupervisor {
    pub fn signal_ready(&self, pid: u32) {
        self.ready.signal(pid);
    }

    pub fn wait_ready(&self, pid: u32, timeout: Duration) -> bool {
        self.ready.wait(pid, timeout)
    }

    pub fn arm_ready_gate(&self, pid: u32) {
        self.ready.arm(pid);
    }

    pub fn cancel_ready_gate(&self, pid: u32) {
        self.ready.cancel(pid);
    }

    pub fn ready_waiter(&self, pid: u32) -> super::readiness::ReadyWaiter {
        self.ready.waiter(pid)
    }

    pub fn video_paper_pid(&self, output: &str) -> Option<u32> {
        lock(&self.video_papers).get(output).map(|(child, _)| child.id())
    }

    pub(crate) fn video_paper_pid_alive(&self, output: &str, pid: u32) -> bool {
        map_entry_pid_alive(&self.video_papers, output, pid)
    }

    pub fn paper_pid(&self) -> Option<u32> {
        lock(&self.paper_child).as_ref().map(Child::id)
    }

    pub(crate) fn paper_pid_alive(&self, pid: u32) -> bool {
        slot_pid_alive(&self.paper_child, pid)
    }

    // A newer apply may replace the paper while the timer sleeps, so the pid check stays inside.
    pub fn retire_paper_after(self: &std::sync::Arc<Self>, pid: u32, delay: Duration) {
        let supervisor = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            let renderer = {
                let mut stdin = lock(&supervisor.paper_stdin);
                let mut child = lock(&supervisor.paper_child);
                if child.as_ref().is_some_and(|process| process.id() == pid) {
                    child.take().map(|process| (process, stdin.take()))
                } else {
                    None
                }
            };
            if let Some(renderer) = renderer {
                super::supervisor::kill_held_renderer(renderer);
            }
        });
    }

    pub fn set_base_still(&self, child: Child, stdin: Option<ChildStdin>) {
        if let Some(mut old) = lock(&self.still_child).take() {
            kill_child(&mut old);
        }
        *lock(&self.still_stdin) = stdin;
        *lock(&self.still_child) = Some(child);
    }

    pub fn has_base_still(&self) -> bool {
        lock(&self.still_stdin).is_some()
    }

    pub(crate) fn base_still_pid_alive(&self, pid: u32) -> bool {
        slot_pid_alive(&self.still_child, pid)
    }

    pub fn renderer_alive(&self, kind: &str) -> bool {
        match kind {
            wall_proto::kind::VIDEO => {
                slot_alive(&self.paper_child) || map_all_alive(&self.video_papers)
            }
            wall_proto::kind::STATIC => {
                slot_alive(&self.still_child) || map_all_alive(&self.output_stills)
            }
            wall_proto::kind::WE => {
                let compatibility_alive = {
                    let mut fleet = lock(&self.fleet);
                    !fleet.is_empty()
                        && fleet.iter_mut().all(|child| matches!(child.try_wait(), Ok(None)))
                };
                compatibility_alive || self.has_scene_papers()
            }
            _ => false,
        }
    }

    pub fn no_per_output_renderers(&self) -> bool {
        lock(&self.output_stills).is_empty() && lock(&self.video_papers).is_empty()
    }

    pub fn kill_base_still(&self) {
        *lock(&self.still_stdin) = None;
        if let Some(mut child) = lock(&self.still_child).take() {
            kill_child(&mut child);
        }
    }

    pub fn take_base_still(&self) -> Option<HeldRenderer> {
        let stdin = lock(&self.still_stdin).take();
        lock(&self.still_child).take().map(|child| (child, stdin))
    }

    pub fn restore_base_still(&self, renderer: HeldRenderer) {
        let (child, stdin) = renderer;
        *lock(&self.still_stdin) = stdin;
        *lock(&self.still_child) = Some(child);
    }

    pub fn take_paper(&self) -> Option<HeldRenderer> {
        let stdin = lock(&self.paper_stdin).take();
        lock(&self.paper_child).take().map(|child| (child, stdin))
    }

    pub fn restore_paper(&self, renderer: HeldRenderer) {
        let (child, mut stdin) = renderer;
        let pause = lock(&self.pause);
        if pause.effective() {
            if let Some(stdin) = stdin.as_mut() {
                Self::write_pause(stdin, true);
            } else {
                Self::signal_pause(&child, true);
            }
        }
        *lock(&self.paper_stdin) = stdin;
        *lock(&self.paper_child) = Some(child);
    }

    pub fn reap_exited_paper(&self) {
        let mut child = lock(&self.paper_child);
        if let Some(process) = child.as_mut()
            && !matches!(process.try_wait(), Ok(None))
        {
            *child = None;
            *lock(&self.paper_stdin) = None;
        }
    }

    pub fn paper_alive(&self) -> bool {
        let mut child = lock(&self.paper_child);
        matches!(child.as_mut().map(Child::try_wait), Some(Ok(None)))
    }

    pub fn reap_exited(&self) {
        lock(&self.fleet).retain_mut(|child| !matches!(child.try_wait(), Ok(Some(_))));
        for slot in [&self.paper_child, &self.still_child] {
            let mut guard = lock(slot);
            if let Some(child) = guard.as_mut()
                && matches!(child.try_wait(), Ok(Some(_)))
            {
                *guard = None;
            }
        }
        reap(&mut lock(&self.output_stills));
        let exited_video = reap(&mut lock(&self.video_papers));
        if !exited_video.is_empty() {
            lock(&self.scene_papers).retain(|output| !exited_video.contains(output));
        }
    }

    pub fn take_video_paper(&self, output: &str) -> Option<HeldRenderer> {
        lock(&self.video_papers).remove(output)
    }

    pub fn restore_video_paper(&self, output: &str, renderer: HeldRenderer) {
        let (child, mut stdin) = renderer;
        let pause = lock(&self.pause);
        if pause.effective() {
            if let Some(stdin) = stdin.as_mut() {
                Self::write_pause(stdin, true);
            } else {
                Self::signal_pause(&child, true);
            }
        }
        lock(&self.video_papers).insert(output.to_string(), (child, stdin));
    }

    pub fn restore_video_paper_state(&self, output: &str, renderer: HeldRenderer, scene: bool) {
        self.restore_video_paper(output, renderer);
        self.mark_scene_paper(output, scene);
    }

    pub fn take_all_video_papers(&self) -> Vec<HeldRenderer> {
        let taken = lock(&self.video_papers).drain().map(|(_, renderer)| renderer).collect();
        lock(&self.scene_papers).clear();
        taken
    }

    pub fn take_video_paper_entries(&self) -> Vec<(String, HeldRenderer, bool)> {
        let scene_papers = lock(&self.scene_papers).clone();
        let taken = lock(&self.video_papers)
            .drain()
            .map(|(output, renderer)| {
                let scene = scene_papers.contains(&output);
                (output, renderer, scene)
            })
            .collect();
        lock(&self.scene_papers).clear();
        taken
    }

    pub fn restore_video_paper_entries(&self, mut renderers: Vec<(String, HeldRenderer, bool)>) {
        let pause = lock(&self.pause);
        for (_, (child, stdin), _) in &mut renderers {
            if pause.effective() {
                if let Some(stdin) = stdin.as_mut() {
                    Self::write_pause(stdin, true);
                } else {
                    Self::signal_pause(child, true);
                }
            }
        }
        let mut restored_scenes = Vec::new();
        let mut video_papers = lock(&self.video_papers);
        for (output, (child, stdin), scene) in renderers {
            if scene {
                restored_scenes.push(output.clone());
            }
            video_papers.insert(output, (child, stdin));
        }
        drop(video_papers);
        lock(&self.scene_papers).extend(restored_scenes);
    }

    pub fn take_video_papers_except(&self, keep: &[String]) -> Vec<HeldRenderer> {
        let mut renderers = lock(&self.video_papers);
        let remove: Vec<String> =
            renderers.keys().filter(|key| !keep.contains(key)).cloned().collect();
        let taken = remove.into_iter().filter_map(|key| renderers.remove(&key)).collect();
        lock(&self.scene_papers).retain(|output| keep.contains(output));
        taken
    }

    pub fn defer_kill_stills(&self, delay_ms: u64) {
        let base = {
            let child = lock(&self.still_child).take();
            let stdin = lock(&self.still_stdin).take();
            child.map(|child| (child, stdin))
        };
        let outputs: Vec<HeldRenderer> =
            lock(&self.output_stills).drain().map(|(_, renderer)| renderer).collect();
        if base.is_none() && outputs.is_empty() {
            return;
        }
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(delay_ms));
            if let Some((mut child, _stdin)) = base {
                kill_child(&mut child);
            }
            for (mut child, _stdin) in outputs {
                kill_child(&mut child);
            }
        });
    }

    pub fn assignments(&self) -> HashMap<String, String> {
        lock(&self.assignments).clone()
    }

    pub fn policy(&self, key: &str) -> Option<String> {
        lock(&self.policies).get(key).cloned()
    }

    pub fn set_policy(&self, key: &str, value: &str) {
        lock(&self.policies).insert(key.to_string(), value.to_string());
    }

    pub fn replace_assignments(&self, assignments: HashMap<String, String>) {
        *lock(&self.assignments) = assignments;
    }

    /// Kills the renderers of outputs that are no longer connected, returning
    /// the outputs it reaped. Dropping an output's assignment leaves its
    /// renderer running against a display that is gone, so the two have to
    /// happen together.
    pub fn kill_departed_outputs(&self, live_outputs: &[String]) -> Vec<String> {
        if live_outputs.is_empty() {
            return Vec::new();
        }
        let departed = |keys: Vec<String>| -> Vec<String> {
            keys.into_iter()
                .filter(|output| {
                    output != "*" && output != "multi" && !live_outputs.contains(output)
                })
                .collect()
        };
        let mut reaped = departed(lock(&self.video_papers).keys().cloned().collect());
        for output in departed(lock(&self.output_stills).keys().cloned().collect()) {
            if !reaped.contains(&output) {
                reaped.push(output);
            }
        }
        for output in &reaped {
            self.kill_video_paper(output);
            self.kill_output_still(output);
        }
        reaped
    }

    pub fn retain_live_output_assignments(&self, live_outputs: &[String]) -> bool {
        let mut assignments = lock(&self.assignments);
        let multi_outputs = assignments
            .get("multi")
            .map(|spec| {
                spec.split(';')
                    .filter_map(|entry| entry.split_once('=').map(|(output, _)| output.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let multi_invalidated = multi_outputs.iter().any(|output| !live_outputs.contains(output));
        assignments.retain(|output, _| {
            output == "*" || output == "multi" || live_outputs.contains(output)
        });
        if multi_invalidated {
            assignments.retain(|output, _| !multi_outputs.contains(output));
            assignments.remove("multi");
        }
        multi_invalidated
    }

    pub fn set_assignment(&self, output: &str, path: &str) {
        lock(&self.assignments).insert(output.to_string(), path.to_string());
    }

    pub fn set_all_assignments(&self, outputs: &[String], path: &str) {
        let mut assignments = lock(&self.assignments);
        assignments.clear();
        for output in outputs {
            assignments.insert(output.clone(), path.to_string());
        }
    }

    pub fn set_assignment_for(&self, output: &str, path: &str, live_outputs: &[String]) {
        if output == "*" {
            self.set_all_assignments(live_outputs, path);
        } else {
            self.set_assignment(output, path);
        }
    }

    pub fn set_output_still(&self, output: &str, child: Child, stdin: Option<ChildStdin>) {
        replace(&mut lock(&self.output_stills), output, child, stdin);
    }

    pub fn take_output_still(&self, output: &str) -> Option<HeldRenderer> {
        lock(&self.output_stills).remove(output)
    }

    pub fn restore_output_still(&self, output: &str, renderer: HeldRenderer) {
        lock(&self.output_stills).insert(output.to_string(), renderer);
    }

    pub fn take_all_output_stills(&self) -> Vec<HeldRenderer> {
        lock(&self.output_stills).drain().map(|(_, renderer)| renderer).collect()
    }

    pub fn kill_output_stills(&self) {
        kill_all(&mut lock(&self.output_stills));
    }

    pub fn retain_output_stills(&self, keep: &[String]) {
        retain(&mut lock(&self.output_stills), keep);
    }

    pub fn set_video_paper(&self, output: &str, child: Child, mut stdin: Option<ChildStdin>) {
        let pause = lock(&self.pause);
        if pause.effective() {
            if let Some(stdin) = stdin.as_mut() {
                Self::write_pause(stdin, true);
            } else {
                Self::signal_pause(&child, true);
            }
        }
        replace(&mut lock(&self.video_papers), output, child, stdin);
    }

    pub fn mark_scene_paper(&self, output: &str, scene: bool) {
        let mut set = lock(&self.scene_papers);
        if scene {
            set.insert(output.to_string());
        } else {
            set.remove(output);
        }
    }

    pub fn is_scene_paper(&self, output: &str) -> bool {
        lock(&self.scene_papers).contains(output)
    }

    pub fn scene_paper_key_for(&self, output: &str) -> Option<String> {
        let scenes = lock(&self.scene_papers);
        if scenes.contains(output) {
            return Some(output.to_string());
        }
        if scenes.contains("*") {
            return Some("*".to_string());
        }
        scenes.iter().find(|key| key.split(',').any(|served| served == output)).cloned()
    }

    pub fn has_scene_papers(&self) -> bool {
        let scenes = lock(&self.scene_papers).iter().cloned().collect::<Vec<_>>();
        scenes.iter().any(|output| self.has_video_paper(output))
    }

    pub fn kill_video_papers(&self) {
        kill_all(&mut lock(&self.video_papers));
        lock(&self.scene_papers).clear();
    }

    pub fn kill_video_paper(&self, output: &str) {
        kill_one(&mut lock(&self.video_papers), output);
        lock(&self.scene_papers).remove(output);
    }

    pub fn kill_output_still(&self, output: &str) {
        kill_one(&mut lock(&self.output_stills), output);
    }

    pub fn has_video_paper(&self, output: &str) -> bool {
        map_entry_alive(&self.video_papers, output)
    }

    pub fn has_output_still(&self, output: &str) -> bool {
        map_entry_alive(&self.output_stills, output)
    }

    pub(crate) fn output_still_pid_alive(&self, output: &str, pid: u32) -> bool {
        map_entry_pid_alive(&self.output_stills, output, pid)
    }

    pub fn retain_video_papers(&self, keep: &[String]) {
        retain(&mut lock(&self.video_papers), keep);
        lock(&self.scene_papers).retain(|output| keep.contains(output));
    }

    pub fn video_paper_outputs(&self) -> Vec<String> {
        lock(&self.video_papers).keys().cloned().collect()
    }

    pub fn swap_paper(&self, child: Child) -> Option<Child> {
        let pause = lock(&self.pause);
        if pause.effective() && lock(&self.paper_stdin).is_none() {
            Self::signal_pause(&child, true);
        }
        let mut slot = lock(&self.paper_child);
        let old = slot.take();
        *slot = Some(child);
        old
    }

    pub fn set_paper_stdin(&self, mut stdin: Option<ChildStdin>) {
        let pause = lock(&self.pause);
        if pause.effective() {
            if let Some(stdin) = stdin.as_mut() {
                Self::write_pause(stdin, true);
            } else if let Some(child) = lock(&self.paper_child).as_ref() {
                Self::signal_pause(child, true);
            }
        }
        *lock(&self.paper_stdin) = stdin;
    }

    pub fn take_paper_stdin(&self) -> Option<ChildStdin> {
        lock(&self.paper_stdin).take()
    }

    pub fn kill_paper(&self) {
        *lock(&self.paper_stdin) = None;
        if let Some(mut child) = lock(&self.paper_child).take() {
            kill_child(&mut child);
        }
    }

    pub fn kill_holders(&self) {
        self.replace_holders(Vec::new());
        *lock(&self.we_render) = WeRender::default();
    }

    pub fn replace_holders(&self, children: Vec<Child>) {
        let pause = lock(&self.pause);
        if pause.effective() {
            for child in &children {
                Self::signal_pause(child, true);
            }
        }
        let children = std::mem::replace(&mut *lock(&self.fleet), children);
        for mut child in children {
            kill_child(&mut child);
        }
    }

    pub fn retain_scene_papers(&self, keep: &[String]) {
        let remove = {
            let mut scenes = lock(&self.scene_papers);
            let remove =
                scenes.iter().filter(|output| !keep.contains(output)).cloned().collect::<Vec<_>>();
            scenes.retain(|output| keep.contains(output));
            remove
        };
        let mut papers = lock(&self.video_papers);
        for output in remove {
            kill_one(&mut papers, &output);
        }
    }

    pub fn we_render_matches(
        &self,
        groups: &BTreeMap<String, Vec<String>>,
        audio: &BTreeMap<String, (bool, u32)>,
    ) -> bool {
        let current = lock(&self.we_render);
        if groups.is_empty() && current.groups.is_empty() {
            return true;
        }
        current.groups == *groups && current.audio == *audio
    }

    pub fn we_render_groups_match(&self, groups: &BTreeMap<String, Vec<String>>) -> bool {
        lock(&self.we_render).groups == *groups
    }

    pub fn set_we_render(
        &self,
        groups: BTreeMap<String, Vec<String>>,
        audio: BTreeMap<String, (bool, u32)>,
    ) {
        *lock(&self.we_render) = WeRender { groups, audio };
    }

    pub fn kill_all(&self) {
        self.kill_paper();
        self.kill_base_still();
        self.kill_output_stills();
        self.kill_video_papers();
        self.kill_holders();
    }

    pub fn track(&self, child: Child) {
        let pause = lock(&self.pause);
        if pause.effective() {
            Self::signal_pause(&child, true);
        }
        let mut fleet = lock(&self.fleet);
        fleet.retain_mut(|child| child.try_wait().map_or(true, |status| status.is_none()));
        fleet.push(child);
    }
}
