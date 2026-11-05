use std::io::Write;
use std::time::Duration;

use anyhow::{Result, anyhow, ensure};
use paper_control::{PaperCommand, StillCommand};

use crate::lock;

use super::supervisor::{PausePolicy, RendererCaptureHandle, RendererSupervisor};

const MULTI_KEY: &str = "multi";
const MULTI_ENTRY_SEPARATOR: &str = ";";
const MULTI_RETAIN_ERROR: &str = "multi video renderer could not retain its outputs";

pub struct RendererApplyWindow<'a> {
    renderers: &'a RendererSupervisor,
}

impl Drop for RendererApplyWindow<'_> {
    fn drop(&mut self) {
        self.renderers.end_apply();
    }
}

impl RendererSupervisor {
    pub fn apply_window(&self) -> RendererApplyWindow<'_> {
        self.begin_apply();
        RendererApplyWindow { renderers: self }
    }

    pub(super) fn write_pause(stdin: &mut std::process::ChildStdin, paused: bool) {
        let line = PaperCommand::pause(paused).line();
        let _ = stdin.write_all(line.as_bytes());
        let _ = stdin.flush();
    }

    pub(super) fn write_duck(stdin: &mut std::process::ChildStdin, ducked: bool) {
        let line = PaperCommand::duck(ducked).line();
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

    fn broadcast_pause_change(&self, before: &PausePolicy, after: &PausePolicy) -> Result<()> {
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
        for (output, (child, stdin)) in lock(&self.video_papers).iter_mut() {
            let is_exempt = exempt.contains(&child.id());
            let was_paused = before.paused_for(is_exempt, output);
            let paused = after.paused_for(is_exempt, output);
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
        self.sync_paper_controls()
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
        let line = self.with_widgets(StillCommand::new(path).with_fill(fill)).line();
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
        let mut renderers = lock(&self.output_stills);
        let Some((_, stdin)) = renderers.get_mut(output) else {
            return false;
        };
        let Some(stdin) = stdin.as_mut() else {
            return false;
        };
        let line = self.with_widgets(command.clone().with_fill(fill)).line();
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

    pub(crate) fn renderer_capture_key(&self, output: &str) -> Option<String> {
        let multi_serves = lock(&self.assignments).get(MULTI_KEY).is_some_and(|specification| {
            specification
                .split(MULTI_ENTRY_SEPARATOR)
                .any(|entry| entry.split_once('=').is_some_and(|(name, _)| name == output))
        });
        let renderers = lock(&self.video_papers);
        let key = if renderers.contains_key(output) {
            Some(output.to_string())
        } else if renderers.contains_key(wall_proto::apply_fields::ALL_OUTPUTS) {
            Some(wall_proto::apply_fields::ALL_OUTPUTS.to_string())
        } else if multi_serves && renderers.contains_key(MULTI_KEY) {
            Some(MULTI_KEY.to_string())
        } else {
            renderers.keys().find(|key| key.split(',').any(|served| served == output)).cloned()
        };
        drop(renderers);
        key.or_else(|| self.has_output_still(output).then(|| output.to_string())).or_else(|| {
            lock(&self.still_child)
                .is_some()
                .then(|| wall_proto::apply_fields::ALL_OUTPUTS.to_string())
        })
    }

    pub(crate) fn capture_renderer_frame(
        &self,
        output: &str,
        path: &str,
        presented: bool,
    ) -> Option<RendererCaptureHandle> {
        let key = self.renderer_capture_key(output)?;
        let mut renderers = lock(&self.video_papers);
        let Some((child, stdin)) = renderers.get_mut(&key) else {
            if !presented {
                return None;
            }
            drop(renderers);
            let mut command = StillCommand::new("");
            command.snapshot =
                Some(paper_control::FrameSnapshot { path: path.into(), output: output.into() });
            let line = command.line();
            let mut stills = lock(&self.output_stills);
            let mut base = lock(&self.still_child);
            let mut base_stdin = lock(&self.still_stdin);
            let (child, stdin) = if let Some((child, stdin)) = stills.get_mut(&key) {
                (child, stdin.as_mut()?)
            } else {
                if key != wall_proto::apply_fields::ALL_OUTPUTS {
                    return None;
                }
                (base.as_mut()?, base_stdin.as_mut()?)
            };
            return stdin
                .write_all(line.as_bytes())
                .and_then(|()| stdin.flush())
                .is_ok()
                .then(|| RendererCaptureHandle { key, pid: child.id(), restores_pause: false });
        };
        let restores_pause = key != MULTI_KEY && !lock(&self.scene_papers).contains(&key);
        let mut command = if presented || !restores_pause {
            PaperCommand::snapshot(path, output)
        } else {
            PaperCommand::freeze(path)
        };
        if restores_pause && presented {
            command.pause = Some(true);
            command.freeze = Some(path.into());
        }
        let line = command.line();
        let stdin = stdin.as_mut()?;
        stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.flush())
            .is_ok()
            .then(|| RendererCaptureHandle { key, pid: child.id(), restores_pause })
    }

    pub(crate) fn capture_scene(&self, key: &str, pid: u32, source: &str, path: &str) -> bool {
        let mut renderers = lock(&self.video_papers);
        let Some((_, Some(stdin))) = renderers.get_mut(key).filter(|(child, _)| child.id() == pid)
        else {
            return false;
        };
        let line = PaperCommand::capture_scene(source, path).line();
        stdin.write_all(line.as_bytes()).and_then(|()| stdin.flush()).is_ok()
    }

    pub(crate) fn finish_renderer_capture(&self, handle: &RendererCaptureHandle) {
        if !handle.restores_pause {
            return;
        }
        let pause = lock(&self.pause);
        let mut renderers = lock(&self.video_papers);
        let Some((child, Some(stdin))) =
            renderers.get_mut(&handle.key).filter(|(child, _)| child.id() == handle.pid)
        else {
            return;
        };
        let paused = pause.policy().paused_for(pause.session_exempt(child.id()), &handle.key);
        let line = PaperCommand::pause(paused).line();
        let _ = stdin.write_all(line.as_bytes());
        let _ = stdin.flush();
    }

    pub(crate) fn renderer_capture_alive(&self, handle: &RendererCaptureHandle) -> bool {
        let mut renderers = lock(&self.video_papers);
        let Some((child, _)) =
            renderers.get_mut(&handle.key).filter(|(child, _)| child.id() == handle.pid)
        else {
            drop(renderers);
            return self.output_still_pid_alive(&handle.key, handle.pid)
                || (handle.key == wall_proto::apply_fields::ALL_OUTPUTS
                    && self.base_still_pid_alive(handle.pid));
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
        let audio = paper_control::AudioSetRequest {
            outputs: filter.unwrap_or_default().to_vec(),
            mute,
            volume,
            ..Default::default()
        };
        let result = crate::infrastructure::paper::PaperClient::existing()
            .and_then(|client| client.map_or(Ok(()), |client| client.audio(audio)));
        if let Err(error) = result {
            log::warn!("paper: audio update failed: {error:#}");
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

    pub fn retire_shared_outputs(&self, selected: &[String]) -> Result<()> {
        self.retire_multi_video_outputs(selected)?;
        let keys = lock(&self.scene_papers).clone();
        for key in keys {
            let outputs = match paper_control::OutputTarget::from_arg(&key) {
                paper_control::OutputTarget::All => {
                    crate::outputs::enumerate().into_iter().map(|output| output.name).collect()
                }
                paper_control::OutputTarget::Named(output) => vec![output],
                paper_control::OutputTarget::Set(outputs) => outputs,
            };
            let remaining = outputs
                .iter()
                .filter(|output| !selected.contains(output))
                .cloned()
                .collect::<Vec<_>>();
            if remaining.len() == outputs.len() {
                continue;
            }
            if remaining.is_empty() {
                self.kill_video_paper(&key);
                continue;
            }
            let mut renderers = lock(&self.video_papers);
            let Some((_, stdin)) = renderers.get_mut(&key) else {
                continue;
            };
            let stdin =
                stdin.as_mut().ok_or_else(|| anyhow!("scene renderer has no control pipe"))?;
            let line = PaperCommand::retain_outputs(&remaining).line();
            stdin.write_all(line.as_bytes()).and_then(|()| stdin.flush())?;
            let replacement = crate::we::scene_renderer_key(&remaining);
            let renderer = renderers.remove(&key).expect("retained scene renderer");
            renderers.insert(replacement.clone(), renderer);
            drop(renderers);
            let mut scenes = lock(&self.scene_papers);
            scenes.remove(&key);
            scenes.insert(replacement.clone());
            drop(scenes);
            let mut policies = lock(&self.policies);
            if let Some(policy) = policies.remove(&format!("scene-fps:{key}")) {
                policies.insert(format!("scene-fps:{replacement}"), policy);
            }
        }
        let mut scenes = lock(&self.we_render);
        scenes.groups.retain(|_, outputs| {
            outputs.retain(|output| !selected.contains(output));
            !outputs.is_empty()
        });
        let groups = scenes.groups.keys().cloned().collect::<std::collections::HashSet<_>>();
        scenes.audio.retain(|key, _| groups.contains(key));
        Ok(())
    }

    fn retire_multi_video_outputs(&self, selected: &[String]) -> Result<()> {
        let mut renderers = lock(&self.video_papers);
        let Some((_, stdin)) = renderers.get_mut(MULTI_KEY) else {
            return Ok(());
        };
        let mut assignments = lock(&self.assignments);
        let specification = assignments
            .get(MULTI_KEY)
            .ok_or_else(|| anyhow!("multi video renderer has no output specification"))?;
        let entries = specification
            .split(MULTI_ENTRY_SEPARATOR)
            .map(|entry| {
                entry
                    .split_once('=')
                    .ok_or_else(|| anyhow!("invalid multi video output specification"))
            })
            .collect::<Result<Vec<_>>>()?;
        let remaining = entries
            .iter()
            .filter(|(output, _)| !selected.iter().any(|target| target == output))
            .collect::<Vec<_>>();
        if remaining.len() == entries.len() {
            return Ok(());
        }
        if remaining.is_empty() {
            drop(assignments);
            drop(renderers);
            self.kill_video_paper(MULTI_KEY);
            lock(&self.assignments).remove(MULTI_KEY);
            return Ok(());
        }
        let stdin =
            stdin.as_mut().ok_or_else(|| anyhow!("multi video renderer has no control pipe"))?;
        let outputs = remaining.iter().map(|(output, _)| (*output).to_string()).collect::<Vec<_>>();
        let line = PaperCommand::retain_outputs(&outputs).line();
        ensure!(
            stdin.write_all(line.as_bytes()).and_then(|()| stdin.flush()).is_ok(),
            MULTI_RETAIN_ERROR
        );
        let specification = remaining
            .iter()
            .map(|(output, path)| format!("{output}={path}"))
            .collect::<Vec<_>>()
            .join(MULTI_ENTRY_SEPARATOR);
        assignments.insert(MULTI_KEY.to_string(), specification);
        Ok(())
    }

    pub fn audio_ducked(&self) -> bool {
        *lock(&self.audio_ducked)
    }

    pub fn set_audio_ducked(&self, ducked: bool) {
        let mut current = lock(&self.audio_ducked);
        if *current == ducked {
            drop(current);
            if self.paper_pending.load(std::sync::atomic::Ordering::Relaxed)
                && let Err(error) = self.sync_paper_controls()
            {
                log::warn!("paper: audio control failed: {error:#}");
            }
            return;
        }
        *current = ducked;
        for (_, stdin) in lock(&self.video_papers).values_mut() {
            if let Some(stdin) = stdin {
                Self::write_duck(stdin, ducked);
            }
        }
        if let Some(stdin) = lock(&self.paper_stdin).as_mut() {
            Self::write_duck(stdin, ducked);
        }
        drop(current);
        if let Err(error) = self.sync_paper_controls() {
            log::warn!("paper: audio control failed: {error:#}");
        }
    }

    pub fn set_automatic_paused(&self, all: bool, outputs: std::collections::HashSet<String>) {
        let mut pause = lock(&self.pause);
        let before = pause.policy();
        pause.automatic = all;
        pause.automatic_outputs = outputs;
        let after = pause.policy();
        drop(pause);
        if (before != after || self.paper_pending.load(std::sync::atomic::Ordering::Relaxed))
            && let Err(error) = self.broadcast_pause_change(&before, &after)
        {
            log::warn!("paper: playback control failed: {error:#}");
        }
    }

    pub fn set_paused(&self, paused: bool) -> Result<()> {
        let mut pause = lock(&self.pause);
        let before = pause.policy();
        pause.manual = paused;
        pause.manual_outputs.clear();
        let after = pause.policy();
        drop(pause);
        self.broadcast_pause_change(&before, &after)
    }

    pub fn set_output_paused(&self, output: &str, paused: bool) -> Result<()> {
        let mut state = lock(&self.pause);
        let before = state.policy();
        state.manual_outputs.insert(output.to_string(), paused);
        let after = state.policy();
        drop(state);
        self.broadcast_pause_change(&before, &after)
    }

    pub fn independent_playback(&self) -> bool {
        lock(&self.pause).independent_playback
    }

    pub fn set_independent_playback(&self, independent: bool) {
        lock(&self.pause).independent_playback = independent;
    }

    pub fn paused_for(&self, output: &str) -> bool {
        lock(&self.pause).policy().paused_for(false, output)
    }

    pub fn manual_paused_for(&self, output: &str) -> bool {
        let pause = lock(&self.pause);
        pause.manual_outputs.get(output).copied().unwrap_or(pause.manual)
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
        if let Err(error) = self.broadcast_pause_change(&before, &after) {
            log::warn!("paper: playback control failed: {error:#}");
        }
    }

    pub fn begin_apply(&self) {
        let mut pause = lock(&self.pause);
        let before = pause.policy();
        pause.applying = pause.applying.saturating_add(1);
        let after = pause.policy();
        drop(pause);
        if before != after
            && let Err(error) = self.broadcast_pause_change(&before, &after)
        {
            log::warn!("paper: playback control failed: {error:#}");
        }
    }

    pub fn end_apply(&self) {
        let mut pause = lock(&self.pause);
        let before = pause.policy();
        pause.applying = pause.applying.saturating_sub(1);
        let after = pause.policy();
        drop(pause);
        if let Err(error) = self.broadcast_pause_change(&before, &after) {
            log::warn!("paper: playback control failed: {error:#}");
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
