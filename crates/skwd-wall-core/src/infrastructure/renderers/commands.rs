use std::io::Write;
use std::time::Duration;

use paper_control::{PaperCommand, StillCommand};

use crate::lock;

use super::supervisor::{PausePolicy, RendererSupervisor, SceneFreezeHandle};

impl RendererSupervisor {
    pub(super) fn write_pause(stdin: &mut std::process::ChildStdin, paused: bool) {
        let line = PaperCommand::pause(paused).line();
        let _ = stdin.write_all(line.as_bytes());
        let _ = stdin.flush();
    }

    pub(super) fn signal_pause(child: &std::process::Child, paused: bool) {
        unsafe {
            libc::kill(
                child.id() as libc::pid_t,
                if paused { libc::SIGSTOP } else { libc::SIGCONT },
            );
        }
    }

    fn broadcast_pause_change(&self, before: PausePolicy, after: PausePolicy) {
        let exempt = {
            let pause = lock(&self.pause);
            pause.session_rendering.keys().copied().collect::<std::collections::HashSet<_>>()
        };
        {
            let mut stdin = lock(&self.paper_stdin);
            let child = lock(&self.paper_child);
            if let Some(child) = child.as_ref() {
                let is_exempt = exempt.contains(&child.id());
                let was_paused = before.paused(is_exempt);
                let paused = after.paused(is_exempt);
                if was_paused != paused {
                    if let Some(stdin) = stdin.as_mut() {
                        Self::write_pause(stdin, paused);
                    } else {
                        Self::signal_pause(child, paused);
                    }
                }
            }
        }
        for (child, stdin) in lock(&self.video_papers).values_mut() {
            let is_exempt = exempt.contains(&child.id());
            let was_paused = before.paused(is_exempt);
            let paused = after.paused(is_exempt);
            if was_paused == paused {
                continue;
            }
            if let Some(stdin) = stdin {
                Self::write_pause(stdin, paused);
            } else {
                Self::signal_pause(child, paused);
            }
        }
        let was_paused = before.paused(false);
        let paused = after.paused(false);
        if was_paused != paused {
            for child in lock(&self.fleet).iter() {
                Self::signal_pause(child, paused);
            }
        }
    }

    fn signal_pause_pid(&self, pid: u32, paused: bool) {
        {
            let mut stdin = lock(&self.paper_stdin);
            let child = lock(&self.paper_child);
            if let Some(child) = child.as_ref().filter(|child| child.id() == pid) {
                if let Some(stdin) = stdin.as_mut() {
                    Self::write_pause(stdin, paused);
                } else {
                    Self::signal_pause(child, paused);
                }
                return;
            }
        }
        for (child, stdin) in lock(&self.video_papers).values_mut() {
            if child.id() != pid {
                continue;
            }
            if let Some(stdin) = stdin {
                Self::write_pause(stdin, paused);
            } else {
                Self::signal_pause(child, paused);
            }
            return;
        }
        if let Some(child) = lock(&self.fleet).iter().find(|child| child.id() == pid) {
            Self::signal_pause(child, paused);
        }
    }

    pub fn still_swap(&self, path: &str, fill: &str) -> bool {
        let mut guard = lock(&self.still_stdin);
        let Some(stdin) = guard.as_mut() else {
            return false;
        };
        let line = StillCommand::new(path).with_fill(fill).line();
        if stdin.write_all(line.as_bytes()).and_then(|()| stdin.flush()).is_ok() {
            true
        } else {
            *guard = None;
            false
        }
    }

    pub fn output_still_swap(&self, output: &str, path: &str, fill: &str) -> bool {
        self.output_still_send(output, &StillCommand::new(path), fill)
    }

    pub fn output_still_swap_slide(
        &self,
        output: &str,
        path: &str,
        direction: &str,
        duration_ms: u64,
        fill: &str,
    ) -> bool {
        self.output_still_send(output, &StillCommand::slide(path, direction, duration_ms), fill)
    }

    pub fn output_still_preload(&self, output: &str, paths: Vec<String>, fill: &str) -> bool {
        if paths.is_empty() {
            return false;
        }
        self.output_still_send(output, &StillCommand::preload(paths), fill)
    }

    fn output_still_send(&self, output: &str, command: &StillCommand, fill: &str) -> bool {
        let command = command.clone().with_fill(fill);
        let mut renderers = lock(&self.output_stills);
        let Some((_, stdin)) = renderers.get_mut(output) else {
            return false;
        };
        let Some(stdin) = stdin.as_mut() else {
            return false;
        };
        let line = command.line();
        stdin.write_all(line.as_bytes()).and_then(|()| stdin.flush()).is_ok()
    }

    pub fn video_swap(&self, output: &str, path: &str, mute: bool, volume: u32) -> bool {
        let mut renderers = lock(&self.video_papers);
        let Some((_, Some(stdin))) = renderers.get_mut(output) else {
            return false;
        };
        let line = PaperCommand::swap_video(path, mute, volume).line();
        stdin.write_all(line.as_bytes()).and_then(|()| stdin.flush()).is_ok()
    }

    pub fn video_swap_fade(
        &self,
        output: &str,
        to: &str,
        shader: &str,
        duration_ms: u64,
        mute: bool,
        volume: u32,
    ) -> bool {
        let mut renderers = lock(&self.video_papers);
        let Some((_, Some(stdin))) = renderers.get_mut(output) else {
            return false;
        };
        let line = PaperCommand::swap_paper(to, shader, duration_ms, mute, volume).line();
        stdin.write_all(line.as_bytes()).and_then(|()| stdin.flush()).is_ok()
    }

    pub fn scene_swap(
        &self,
        output: &str,
        dir: &str,
        mute: bool,
        volume: u32,
        properties: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> bool {
        self.write_paper_line(
            output,
            &PaperCommand::swap_video(dir, mute, volume).with_properties(properties.cloned()),
        )
    }

    pub fn scene_swap_fade(
        &self,
        output: &str,
        to: &str,
        shader: &str,
        duration_ms: u64,
        mute: bool,
        volume: u32,
        properties: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> bool {
        self.write_paper_line(
            output,
            &PaperCommand::swap_paper(to, shader, duration_ms, mute, volume)
                .with_properties(properties.cloned()),
        )
    }

    pub(crate) fn freeze_scene(&self, output: &str, path: &str) -> Option<SceneFreezeHandle> {
        let key = self.scene_paper_key_for(output)?;
        let mut renderers = lock(&self.video_papers);
        let (child, stdin) = renderers.get_mut(&key)?;
        let line = PaperCommand::freeze(path).line();
        let stdin = stdin.as_mut()?;
        stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.flush())
            .is_ok()
            .then(|| SceneFreezeHandle { key, pid: child.id() })
    }

    pub(crate) fn finish_scene_freeze(&self, handle: &SceneFreezeHandle) {
        // Renderer installation already takes pause before video_papers. Keep
        // the same global order while restoring the post-capture policy.
        let pause = lock(&self.pause);
        let mut renderers = lock(&self.video_papers);
        let Some((child, Some(stdin))) =
            renderers.get_mut(&handle.key).filter(|(child, _)| child.id() == handle.pid)
        else {
            return;
        };
        let paused = pause.policy().paused(pause.session_exempt(child.id()));
        let line = PaperCommand::pause(paused).line();
        let _ = stdin.write_all(line.as_bytes());
        let _ = stdin.flush();
    }

    pub(crate) fn scene_freeze_alive(&self, handle: &SceneFreezeHandle) -> bool {
        let mut renderers = lock(&self.video_papers);
        let Some((child, _)) =
            renderers.get_mut(&handle.key).filter(|(child, _)| child.id() == handle.pid)
        else {
            return false;
        };
        matches!(child.try_wait(), Ok(None))
    }

    fn write_paper_line(&self, output: &str, command: &PaperCommand) -> bool {
        let mut renderers = lock(&self.video_papers);
        let Some((_, Some(stdin))) = renderers.get_mut(output) else {
            return false;
        };
        let line = command.line();
        stdin.write_all(line.as_bytes()).and_then(|()| stdin.flush()).is_ok()
    }

    pub fn paper_swap(
        &self,
        to: &str,
        shader: &str,
        duration_ms: u64,
        mute: bool,
        volume: u32,
    ) -> bool {
        let line = PaperCommand::swap_paper(to, shader, duration_ms, mute, volume).line();
        let mut stdin = lock(&self.paper_stdin);
        let Some(writer) = stdin.as_mut() else {
            return false;
        };
        if writer.write_all(line.as_bytes()).and_then(|()| writer.flush()).is_ok() {
            true
        } else {
            *stdin = None;
            false
        }
    }

    pub fn send_audio(&self, filter: Option<&[String]>, mute: Option<bool>, volume: Option<u32>) {
        let line = PaperCommand::audio(mute, volume).line();
        let mut renderers = lock(&self.video_papers);
        let outputs: Vec<String> = renderers.keys().cloned().collect();
        for output in outputs {
            if output == "multi" {
                let command = filter.map_or_else(
                    || PaperCommand::audio(mute, volume),
                    |targets| PaperCommand::audio_for(targets, mute, volume),
                );
                if let Some((_, Some(stdin))) = renderers.get_mut(&output) {
                    let _ = stdin.write_all(command.line().as_bytes());
                    let _ = stdin.flush();
                }
                continue;
            }
            if let Some(filter) = filter
                && output != "*"
                && !output.split(',').any(|served| filter.iter().any(|name| name == served))
            {
                continue;
            }
            if let Some((_, Some(stdin))) = renderers.get_mut(&output) {
                let _ = stdin.write_all(line.as_bytes());
                let _ = stdin.flush();
            }
        }
        drop(renderers);
        if let Some(stdin) = lock(&self.paper_stdin).as_mut() {
            let _ = stdin.write_all(line.as_bytes());
            let _ = stdin.flush();
        }
    }

    pub fn send_shared_video_audio(&self, mute: bool, volume: u32) {
        let line = PaperCommand::audio(Some(mute), Some(volume)).line();
        if let Some((_, Some(stdin))) = lock(&self.video_papers).get_mut("*") {
            let _ = stdin.write_all(line.as_bytes());
            let _ = stdin.flush();
        }
    }

    pub fn send_multi_video_audio(&self, outputs: &[String], mute: bool, volume: u32) {
        let line = PaperCommand::audio_for(outputs, Some(mute), Some(volume)).line();
        if let Some((_, Some(stdin))) = lock(&self.video_papers).get_mut("multi") {
            let _ = stdin.write_all(line.as_bytes());
            let _ = stdin.flush();
        }
    }

    pub fn set_paused(&self, paused: bool) {
        let mut pause = lock(&self.pause);
        let before = pause.policy();
        pause.manual = paused;
        let after = pause.policy();
        drop(pause);
        if before == after {
            return;
        }
        self.broadcast_pause_change(before, after);
    }

    pub fn paused(&self) -> bool {
        lock(&self.pause).effective()
    }

    pub fn set_session_paused(&self, session_id: u64, paused: bool) {
        let mut pause = lock(&self.pause);
        let before = pause.policy();
        if paused {
            pause.sessions.insert(session_id);
        } else {
            pause.sessions.remove(&session_id);
        }
        let after = pause.policy();
        drop(pause);
        if before == after {
            return;
        }
        self.broadcast_pause_change(before, after);
    }

    pub fn begin_apply(&self) {
        let mut pause = lock(&self.pause);
        let before = pause.policy();
        pause.applying = pause.applying.saturating_add(1);
        let after = pause.policy();
        drop(pause);
        if before != after {
            self.broadcast_pause_change(before, after);
        }
    }

    pub fn end_apply(&self) {
        let mut pause = lock(&self.pause);
        let before = pause.policy();
        pause.applying = pause.applying.saturating_sub(1);
        let after = pause.policy();
        drop(pause);
        if before != after {
            self.broadcast_pause_change(before, after);
        }
    }

    pub fn allow_session_rendering_for(self: &std::sync::Arc<Self>, pid: u32, duration: Duration) {
        let (token, resume) = {
            let mut pause = lock(&self.pause);
            let policy = pause.policy();
            let was_exempt = pause.session_exempt(pid);
            pause.next_session_rendering = pause.next_session_rendering.wrapping_add(1).max(1);
            let token = pause.next_session_rendering;
            pause.session_rendering.insert(pid, token);
            (token, policy.paused(was_exempt) != policy.paused(true))
        };
        if resume {
            self.signal_pause_pid(pid, false);
        }
        let supervisor = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(duration);
            let pause_renderer = {
                let mut pause = lock(&supervisor.pause);
                if pause.session_rendering.get(&pid) != Some(&token) {
                    return;
                }
                let policy = pause.policy();
                let was_paused = policy.paused(true);
                pause.session_rendering.remove(&pid);
                let paused = policy.paused(false);
                (was_paused != paused).then_some(paused)
            };
            if let Some(paused) = pause_renderer {
                supervisor.signal_pause_pid(pid, paused);
            }
        });
    }
}
