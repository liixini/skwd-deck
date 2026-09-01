use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::Context;

use crate::domain::wallpaper::still_args;
use crate::infrastructure::renderers::{HeldRenderer, kill_held_renderer};
use crate::state::WallState;

pub(crate) const READY_TIMEOUT: Duration = Duration::from_millis(3000);
pub(crate) const NATIVE_SCENE_READY_TIMEOUT: Duration = Duration::from_secs(10);
pub(super) const PERF_SCENE_FPS: u32 = 30;
pub(super) const PERF_SCENE_MAX_DIMENSION: u32 = 2048;
pub(super) const PERF_SCENE_EFFECT_CHAINS: usize = 4;
pub(super) const PERF_SCENE_EFFECT_PASSES: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NativeScenePolicy {
    pub fill_mode: String,
    pub assets_dir: String,
    pub fps: u32,
    pub disable_particles: bool,
    pub max_dimension: Option<u32>,
    pub effect_chains: Option<usize>,
    pub effect_passes: Option<usize>,
}

impl NativeScenePolicy {
    pub(super) fn signature(&self) -> String {
        format!(
            "v6:{}:{}:{}:{}:{}:{}:{}",
            self.fill_mode,
            self.assets_dir,
            self.fps,
            self.disable_particles,
            self.max_dimension.unwrap_or(0),
            self.effect_chains.unwrap_or(0),
            self.effect_passes.unwrap_or(0),
        )
    }
}

pub(super) fn native_scene_policy(
    configured_fps: u32,
    performance_mode: bool,
    disable_particles: bool,
) -> NativeScenePolicy {
    NativeScenePolicy {
        fill_mode: String::new(),
        assets_dir: String::new(),
        fps: if performance_mode { configured_fps.min(PERF_SCENE_FPS) } else { configured_fps },
        disable_particles,
        max_dimension: performance_mode.then_some(PERF_SCENE_MAX_DIMENSION),
        effect_chains: performance_mode.then_some(PERF_SCENE_EFFECT_CHAINS),
        effect_passes: performance_mode.then_some(PERF_SCENE_EFFECT_PASSES),
    }
}

pub(super) fn current_native_scene_policy(state: &WallState) -> NativeScenePolicy {
    let mut policy = native_scene_policy(
        state.config().renderer().we_fps(),
        state.config().renderer().performance_mode(),
        state.config().renderer().we_disable_particles(),
    );
    policy.fill_mode = state.config().renderer().we_scene_fill_mode();
    policy.assets_dir = state.config().we_assets_dir();
    policy
}

pub(super) fn apply_native_scene_policy(cmd: &mut Command, policy: &NativeScenePolicy) {
    cmd.env("SKWD_PAPER_WE_FPS", policy.fps.to_string());
    cmd.env("SKWD_PAPER_WE_DISABLE_PARTICLES", if policy.disable_particles { "1" } else { "0" });
    if !policy.assets_dir.is_empty() {
        cmd.env("SKWD_WE_ASSETS", &policy.assets_dir);
    }
    if let Some(max_dimension) = policy.max_dimension {
        cmd.env("SKWD_VK_SCENE_MAX", max_dimension.to_string());
    }
    if let Some(effect_chains) = policy.effect_chains {
        cmd.env("SKWD_VK_SCENE_FX", effect_chains.to_string());
    }
    if let Some(effect_passes) = policy.effect_passes {
        cmd.env("SKWD_VK_FX_PASSES", effect_passes.to_string());
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RendererLaunchKind {
    SharedStatic,
    PerOutputStatic { output: String },
    MultiOutputStatic { outputs: String },
    SharedVideo,
    PerOutputVideo { output: String },
    MultiOutputVideo,
    NativeScene { outputs: String },
    ManagedTransition { output: String },
    StandaloneTransition { output: String },
}

impl RendererLaunchKind {
    fn output(&self) -> &str {
        match self {
            Self::SharedStatic | Self::SharedVideo => "*",
            Self::PerOutputStatic { output }
            | Self::MultiOutputStatic { outputs: output }
            | Self::PerOutputVideo { output }
            | Self::NativeScene { outputs: output }
            | Self::ManagedTransition { output }
            | Self::StandaloneTransition { output } => output,
            Self::MultiOutputVideo => "multi",
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::SharedStatic => "shared static renderer",
            Self::PerOutputStatic { .. } => "per-output static renderer",
            Self::MultiOutputStatic { .. } => "multi-output static renderer",
            Self::SharedVideo => "shared video renderer",
            Self::PerOutputVideo { .. } => "per-output video renderer",
            Self::MultiOutputVideo => "multi-output video renderer",
            Self::NativeScene { .. } => "native scene renderer",
            Self::ManagedTransition { .. } => "managed transition renderer",
            Self::StandaloneTransition { .. } => "standalone transition renderer",
        }
    }

    fn executable(&self, state: &WallState) -> String {
        match self {
            Self::SharedStatic | Self::PerOutputStatic { .. } | Self::MultiOutputStatic { .. } => {
                state.config().renderer().still_bin()
            }
            Self::SharedVideo
            | Self::PerOutputVideo { .. }
            | Self::MultiOutputVideo
            | Self::NativeScene { .. }
            | Self::ManagedTransition { .. }
            | Self::StandaloneTransition { .. } => state.config().renderer().vk_bin(),
        }
    }

    fn is_steady(&self) -> bool {
        matches!(
            self,
            Self::SharedStatic
                | Self::PerOutputStatic { .. }
                | Self::MultiOutputStatic { .. }
                | Self::SharedVideo
                | Self::PerOutputVideo { .. }
                | Self::MultiOutputVideo
                | Self::NativeScene { .. }
        )
    }

    fn is_native_scene(&self) -> bool {
        matches!(self, Self::NativeScene { .. })
    }

    fn is_standalone(&self) -> bool {
        matches!(self, Self::StandaloneTransition { .. })
    }

    fn timeout(&self) -> Duration {
        if self.is_native_scene() { NATIVE_SCENE_READY_TIMEOUT } else { READY_TIMEOUT }
    }

    fn target(&self) -> RendererTarget {
        match self {
            Self::SharedStatic => RendererTarget::BaseStill,
            Self::PerOutputStatic { output } | Self::MultiOutputStatic { outputs: output } => {
                RendererTarget::OutputStill(output.clone())
            }
            Self::SharedVideo => RendererTarget::SharedVideo,
            Self::PerOutputVideo { output } => {
                RendererTarget::VideoPaper { output: output.clone(), role: VideoPaperRole::Video }
            }
            Self::MultiOutputVideo => RendererTarget::VideoPaper {
                output: "multi".to_string(),
                role: VideoPaperRole::Video,
            },
            Self::NativeScene { outputs } => RendererTarget::VideoPaper {
                output: outputs.clone(),
                role: VideoPaperRole::NativeScene,
            },
            Self::ManagedTransition { .. } => RendererTarget::Paper,
            Self::StandaloneTransition { .. } => RendererTarget::Detached,
        }
    }
}

pub(crate) struct RendererLaunchSpec {
    kind: RendererLaunchKind,
    arguments: Vec<String>,
}

impl RendererLaunchSpec {
    fn control_stdin(&self) -> bool {
        !self.kind.is_standalone()
            || self.arguments.iter().any(|argument| argument == "--transition-hold")
    }

    pub(crate) fn static_for(output: &str, path: &str, fill_mode: &str) -> Self {
        let kind = if output == "*" {
            RendererLaunchKind::SharedStatic
        } else if output.contains(',') {
            RendererLaunchKind::MultiOutputStatic { outputs: output.to_string() }
        } else {
            RendererLaunchKind::PerOutputStatic { output: output.to_string() }
        };
        let mut arguments = still_args(output, path, fill_mode);
        arguments.push("--persist".to_string());
        Self { kind, arguments }
    }

    pub(crate) fn video_for(output: &str, arguments: Vec<String>) -> Self {
        let kind = if output == "*" {
            RendererLaunchKind::SharedVideo
        } else if output == "multi" {
            RendererLaunchKind::MultiOutputVideo
        } else {
            RendererLaunchKind::PerOutputVideo { output: output.to_string() }
        };
        Self { kind, arguments }
    }

    pub(crate) fn native_scene(outputs: &str, arguments: Vec<String>) -> Self {
        Self { kind: RendererLaunchKind::NativeScene { outputs: outputs.to_string() }, arguments }
    }

    pub(crate) fn managed_transition(arguments: Vec<String>) -> Self {
        let output = arguments.first().cloned().unwrap_or_else(|| "*".to_string());
        Self { kind: RendererLaunchKind::ManagedTransition { output }, arguments }
    }

    pub(crate) fn standalone_transition(output: &str, mut arguments: Vec<String>) -> Self {
        if !arguments.iter().any(|argument| argument == "--standalone") {
            arguments.push("--standalone".to_string());
        }
        Self {
            kind: RendererLaunchKind::StandaloneTransition { output: output.to_string() },
            arguments,
        }
    }

    pub(crate) fn staged_transition(output: &str, mut arguments: Vec<String>) -> Self {
        arguments.push("--transition-hold".to_string());
        Self {
            kind: RendererLaunchKind::StandaloneTransition { output: output.to_string() },
            arguments,
        }
    }

    fn command(&self, state: &WallState) -> Command {
        let executable = self.kind.executable(state);
        let outputs = crate::outputs::enumerate();
        let config = state.config();
        let transition_fps = config
            .transition()
            .sand_fps()
            .parse()
            .ok()
            .map(|limit| crate::outputs::target_fps(limit, self.kind.output(), &outputs));
        let shader = config.transition().shader();
        let sand_scope = if shader.starts_with("sand-") {
            config.transition().scope(&shader)
        } else {
            String::from("all")
        };
        let mut command = crate::proc::renderer(&executable);
        command
            .args(&self.arguments)
            .env("SKWD_PAPER_READY_SOCKET", wall_proto::resolve_socket())
            .env("SKWD_PAPER_SAND_QUALITY", config.transition().sand_quality())
            .env("SKWD_PAPER_SAND_SCOPE", sand_scope)
            .env("SKWD_PAPER_SAND_PRIMARY", config.transition().sand_primary())
            .env("SKWD_PAPER_SAND_SHARP", if config.transition().sand_sharp() { "1" } else { "0" })
            .env(
                "SKWD_PAPER_SAND_FPS",
                transition_fps.map_or_else(|| "auto".to_string(), |fps| fps.to_string()),
            )
            .stdin(if self.control_stdin() { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::null())
            .stderr(if self.kind.is_native_scene() { Stdio::inherit() } else { Stdio::null() });
        if self.kind.is_steady() {
            command
                .env("SKWD_PAPER_IDLE_SEC", config.renderer().idle_pause_seconds().to_string())
                .env(
                    "SKWD_PAPER_TRANSITIONS",
                    if config.transition().active() { "1" } else { "0" },
                );
        }
        if self.kind.is_native_scene() {
            let mut policy = current_native_scene_policy(state);
            policy.fps = policy.fps.min(crate::outputs::target_fps(
                config.renderer().we_fps(),
                self.kind.output(),
                &outputs,
            ));
            command.env(
                "SKWD_PAPER_OUTPUT_FPS",
                crate::outputs::fps_map(config.renderer().we_fps(), &outputs),
            );
            apply_native_scene_policy(&mut command, &policy);
        }
        command
    }

    pub(crate) fn spawn(self, state: &WallState) -> anyhow::Result<RendererStartup<'_>> {
        let executable = self.kind.executable(state);
        log::debug!(
            "{} spawn ({}): {executable} {}",
            self.kind.label(),
            self.kind.output(),
            self.arguments.join(" ")
        );
        let target = self.kind.target();
        let mut displaced = DisplacedRenderers::take(state, &target);
        let mut child = match self.command(state).spawn() {
            Ok(child) => child,
            Err(error) => {
                displaced.restore(state);
                return Err(error).with_context(|| format!("spawn {executable}"));
            }
        };
        let pid = child.id();
        let stdin = child.stdin.take();
        let detached = target.install(state, child, stdin);
        Ok(RendererStartup {
            transaction: RendererTransaction {
                state,
                target,
                displaced,
                detached,
                pid,
                timeout: self.kind.timeout(),
                label: self.kind.label(),
                committed: false,
            },
        })
    }
}

#[derive(Debug)]
enum RendererTarget {
    BaseStill,
    OutputStill(String),
    SharedVideo,
    VideoPaper { output: String, role: VideoPaperRole },
    Paper,
    Detached,
}

#[derive(Debug, Clone, Copy)]
enum VideoPaperRole {
    Video,
    NativeScene,
}

impl VideoPaperRole {
    fn from_scene(scene: bool) -> Self {
        if scene { Self::NativeScene } else { Self::Video }
    }

    fn is_scene(self) -> bool {
        matches!(self, Self::NativeScene)
    }
}

impl RendererTarget {
    fn install(
        &self,
        state: &WallState,
        child: std::process::Child,
        stdin: Option<std::process::ChildStdin>,
    ) -> Option<HeldRenderer> {
        match self {
            Self::BaseStill => state.renderers().restore_base_still((child, stdin)),
            Self::OutputStill(output) => {
                state.renderers().restore_output_still(output, (child, stdin));
            }
            Self::SharedVideo => {
                state.renderers().restore_video_paper_state("*", (child, stdin), false);
            }
            Self::VideoPaper { output, role } => {
                state.renderers().restore_video_paper_state(
                    output,
                    (child, stdin),
                    role.is_scene(),
                );
            }
            Self::Paper => state.renderers().restore_paper((child, stdin)),
            Self::Detached => return Some((child, stdin)),
        }
        None
    }

    fn take_candidate(&self, state: &WallState) -> Option<HeldRenderer> {
        match self {
            Self::BaseStill => state.renderers().take_base_still(),
            Self::OutputStill(output) => state.renderers().take_output_still(output),
            Self::SharedVideo => {
                let renderer = state.renderers().take_video_paper("*");
                state.renderers().mark_scene_paper("*", false);
                renderer
            }
            Self::VideoPaper { output, .. } => {
                let renderer = state.renderers().take_video_paper(output);
                state.renderers().mark_scene_paper(output, false);
                renderer
            }
            Self::Paper => state.renderers().take_paper(),
            Self::Detached => None,
        }
    }

    fn candidate_alive(&self, state: &WallState, pid: u32) -> bool {
        match self {
            Self::BaseStill => state.renderers().base_still_pid_alive(pid),
            Self::OutputStill(output) => state.renderers().output_still_pid_alive(output, pid),
            Self::SharedVideo => state.renderers().video_paper_pid_alive("*", pid),
            Self::VideoPaper { output, .. } => state.renderers().video_paper_pid_alive(output, pid),
            Self::Paper => state.renderers().paper_pid_alive(pid),
            Self::Detached => false,
        }
    }

    fn restore_unrelated(&self, state: &WallState, renderer: HeldRenderer) {
        match self {
            Self::BaseStill => state.renderers().restore_base_still(renderer),
            Self::OutputStill(output) => state.renderers().restore_output_still(output, renderer),
            Self::SharedVideo => {
                state.renderers().restore_video_paper_state("*", renderer, false);
            }
            Self::VideoPaper { output, role } => {
                state.renderers().restore_video_paper_state(output, renderer, role.is_scene());
            }
            Self::Paper => state.renderers().restore_paper(renderer),
            Self::Detached => kill_held_renderer(renderer),
        }
    }
}

enum DisplacedRenderers {
    None,
    BaseStill(Option<HeldRenderer>),
    OutputStill { output: String, renderer: Option<HeldRenderer> },
    SharedVideo { videos: Vec<DisplacedVideoPaper>, paper: Option<HeldRenderer> },
    VideoPaper { output: String, renderer: Option<HeldRenderer>, role: VideoPaperRole },
    Paper(Option<HeldRenderer>),
}

struct DisplacedVideoPaper {
    output: String,
    renderer: HeldRenderer,
    role: VideoPaperRole,
}

impl DisplacedRenderers {
    fn take(state: &WallState, target: &RendererTarget) -> Self {
        match target {
            RendererTarget::BaseStill => Self::BaseStill(state.renderers().take_base_still()),
            RendererTarget::OutputStill(output) => Self::OutputStill {
                output: output.clone(),
                renderer: state.renderers().take_output_still(output),
            },
            RendererTarget::SharedVideo => Self::SharedVideo {
                videos: state
                    .renderers()
                    .take_video_paper_entries()
                    .into_iter()
                    .map(|(output, renderer, scene)| DisplacedVideoPaper {
                        output,
                        renderer,
                        role: VideoPaperRole::from_scene(scene),
                    })
                    .collect(),
                paper: state.renderers().take_paper(),
            },
            RendererTarget::VideoPaper { output, .. } => {
                let scene = state.renderers().is_scene_paper(output);
                Self::VideoPaper {
                    output: output.clone(),
                    renderer: state.renderers().take_video_paper(output),
                    role: VideoPaperRole::from_scene(scene),
                }
            }
            RendererTarget::Paper => Self::Paper(state.renderers().take_paper()),
            RendererTarget::Detached => Self::None,
        }
    }

    fn any(&self) -> bool {
        match self {
            Self::None => false,
            Self::BaseStill(renderer) | Self::Paper(renderer) => renderer.is_some(),
            Self::OutputStill { renderer, .. } | Self::VideoPaper { renderer, .. } => {
                renderer.is_some()
            }
            Self::SharedVideo { videos, paper } => !videos.is_empty() || paper.is_some(),
        }
    }

    fn restore(&mut self, state: &WallState) {
        match std::mem::replace(self, Self::None) {
            Self::BaseStill(Some(renderer)) => state.renderers().restore_base_still(renderer),
            Self::OutputStill { output, renderer: Some(renderer) } => {
                state.renderers().restore_output_still(&output, renderer);
            }
            Self::SharedVideo { videos, paper } => {
                state.renderers().restore_video_paper_entries(
                    videos
                        .into_iter()
                        .map(|video| (video.output, video.renderer, video.role.is_scene()))
                        .collect(),
                );
                if let Some(renderer) = paper {
                    state.renderers().restore_paper(renderer);
                }
            }
            Self::VideoPaper { output, renderer: Some(renderer), role } => {
                state.renderers().restore_video_paper_state(&output, renderer, role.is_scene());
            }
            Self::Paper(Some(renderer)) => state.renderers().restore_paper(renderer),
            Self::None
            | Self::BaseStill(None)
            | Self::OutputStill { renderer: None, .. }
            | Self::VideoPaper { renderer: None, .. }
            | Self::Paper(None) => {}
        }
    }

    fn retire(&mut self) {
        match std::mem::replace(self, Self::None) {
            Self::None
            | Self::BaseStill(None)
            | Self::OutputStill { renderer: None, .. }
            | Self::VideoPaper { renderer: None, .. }
            | Self::Paper(None) => {}
            Self::BaseStill(Some(renderer))
            | Self::Paper(Some(renderer))
            | Self::OutputStill { renderer: Some(renderer), .. }
            | Self::VideoPaper { renderer: Some(renderer), .. } => {
                kill_held_renderer(renderer);
            }
            Self::SharedVideo { videos, paper } => {
                for video in videos {
                    kill_held_renderer(video.renderer);
                }
                if let Some(renderer) = paper {
                    kill_held_renderer(renderer);
                }
            }
        }
    }
}

struct RendererTransaction<'a> {
    state: &'a WallState,
    target: RendererTarget,
    displaced: DisplacedRenderers,
    detached: Option<HeldRenderer>,
    pid: u32,
    timeout: Duration,
    label: &'static str,
    committed: bool,
}

impl RendererTransaction<'_> {
    fn candidate_alive(&mut self) -> bool {
        match self.detached.as_mut() {
            Some((child, _)) if child.id() == self.pid => match child.try_wait() {
                Ok(None) => true,
                Ok(Some(status)) => status.success(),
                Err(_) => false,
            },
            Some(_) => false,
            None => self.target.candidate_alive(self.state, self.pid),
        }
    }

    fn retire_displaced(&mut self) {
        self.displaced.retire();
        if let Some((mut child, stdin)) = self.detached.take() {
            std::thread::spawn(move || {
                let deadline = std::time::Instant::now() + DETACHED_OVERLAY_LINGER;
                while matches!(child.try_wait(), Ok(None)) && std::time::Instant::now() < deadline {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                drop(stdin);
                let _ = child.wait();
            });
        }
        self.committed = true;
    }
}

const DETACHED_OVERLAY_LINGER: std::time::Duration = std::time::Duration::from_secs(30);

impl Drop for RendererTransaction<'_> {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        self.state.renderers().cancel_ready_gate(self.pid);
        let candidate = self.detached.take().or_else(|| self.target.take_candidate(self.state));
        if let Some(renderer) = candidate {
            if renderer.0.id() == self.pid {
                kill_held_renderer(renderer);
            } else {
                self.target.restore_unrelated(self.state, renderer);
            }
        }
        self.displaced.restore(self.state);
    }
}

pub(crate) struct RendererStartup<'a> {
    transaction: RendererTransaction<'a>,
}

impl<'a> RendererStartup<'a> {
    #[cfg(test)]
    pub(crate) fn pid(&self) -> u32 {
        self.transaction.pid
    }

    pub(crate) fn has_displaced(&self) -> bool {
        self.transaction.displaced.any()
    }

    pub(crate) fn wait_ready(self) -> anyhow::Result<ReadyRenderer<'a>> {
        let mut transaction = self.transaction;
        if transaction.state.renderers().wait_ready(transaction.pid, transaction.timeout)
            && transaction.candidate_alive()
        {
            Ok(ReadyRenderer { transaction })
        } else {
            anyhow::bail!("{} did not become ready and remain alive", transaction.label)
        }
    }
}

pub(crate) struct ReadyRenderer<'a> {
    transaction: RendererTransaction<'a>,
}

impl<'a> ReadyRenderer<'a> {
    pub(crate) fn pid(&self) -> u32 {
        self.transaction.pid
    }

    pub(crate) fn prepare_commit(mut self) -> anyhow::Result<PreparedRenderer<'a>> {
        if !self.transaction.candidate_alive() {
            anyhow::bail!("{} exited before commit", self.transaction.label);
        }
        Ok(PreparedRenderer { transaction: self.transaction })
    }

    pub(crate) fn commit(self) -> anyhow::Result<u32> {
        Ok(self.prepare_commit()?.finalize())
    }
}

pub(crate) struct PreparedRenderer<'a> {
    // The displaced renderer remains owned here until the whole batch can finalize.
    transaction: RendererTransaction<'a>,
}

impl PreparedRenderer<'_> {
    pub(crate) fn start_transition(&mut self) -> anyhow::Result<()> {
        let stdin = self
            .transaction
            .detached
            .as_mut()
            .and_then(|(_, stdin)| stdin.as_mut())
            .context("staged transition has no control stdin")?;
        stdin
            .write_all(paper_control::PaperCommand::pause(false).line().as_bytes())
            .and_then(|()| stdin.flush())
            .context("release staged transition")
    }

    pub(crate) fn finalize(mut self) -> u32 {
        let pid = self.transaction.pid;
        self.transaction.retire_displaced();
        pid
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::path::Path;

    use super::*;

    fn command_env(command: &Command, key: &str) -> Option<String> {
        command
            .get_envs()
            .find(|(name, _)| *name == OsStr::new(key))
            .and_then(|(_, value)| value)
            .map(|value| value.to_string_lossy().into_owned())
    }

    fn sleeper() -> std::process::Child {
        Command::new("sleep").arg("60").spawn().expect("spawn sleeper")
    }

    fn executable(path: &Path) {
        std::fs::write(path, b"#!/bin/sh\nexec cat\n").unwrap();
        let mut permissions = std::fs::metadata(path).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
        std::fs::set_permissions(path, permissions).unwrap();
    }

    #[test]
    fn named_roles_resolve_executable_control_and_timeout() {
        let state = WallState::test_new(serde_json::json!({
            "paths": {"paperStillBin": "/bin/still", "paperVkBin": "/bin/vk"}
        }));
        let still = RendererLaunchSpec::static_for("DP-1", "/w/a.png", "fill");
        let scene = RendererLaunchSpec::native_scene(
            "DP-1,DP-2",
            vec!["DP-1,DP-2".into(), "/we/42".into(), "--scene".into(), "/we/42".into()],
        );
        let standalone = RendererLaunchSpec::standalone_transition(
            "DP-1",
            vec!["DP-1".into(), "/w/b.png".into()],
        );
        let staged =
            RendererLaunchSpec::staged_transition("DP-1", vec!["DP-1".into(), "/w/c.png".into()]);
        assert_eq!(still.command(&state).get_program(), OsStr::new("/bin/still"));
        assert_eq!(scene.command(&state).get_program(), OsStr::new("/bin/vk"));
        assert_eq!(scene.kind.timeout(), NATIVE_SCENE_READY_TIMEOUT);
        assert_eq!(still.kind.timeout(), READY_TIMEOUT);
        assert!(standalone.arguments.iter().any(|argument| argument == "--standalone"));
        assert!(staged.arguments.iter().any(|argument| argument == "--transition-hold"));
        assert!(staged.control_stdin());
        assert!(!standalone.control_stdin());
        assert!(matches!(standalone.kind, RendererLaunchKind::StandaloneTransition { .. }));
    }

    #[test]
    fn steady_and_scene_environment_are_owned_by_spec() {
        let state = WallState::test_new(serde_json::json!({
            "paths": {"paperStillBin": "/bin/still", "paperVkBin": "/bin/vk"},
            "weRender": {"fps": 75}
        }));
        let steady =
            RendererLaunchSpec::video_for("*", vec!["*".into(), "/v/a.mp4".into()]).command(&state);
        let transition =
            RendererLaunchSpec::managed_transition(vec!["*".into(), "/w/b.png".into()])
                .command(&state);
        let scene = RendererLaunchSpec::native_scene(
            "DP-1",
            vec!["DP-1".into(), "/we/42".into(), "--scene".into(), "/we/42".into()],
        )
        .command(&state);
        for key in [
            "SKWD_PAPER_SAND_QUALITY",
            "SKWD_PAPER_SAND_SCOPE",
            "SKWD_PAPER_SAND_PRIMARY",
            "SKWD_PAPER_SAND_SHARP",
            "SKWD_PAPER_SAND_FPS",
        ] {
            assert!(command_env(&steady, key).is_some(), "{key}");
        }
        assert!(command_env(&steady, "SKWD_PAPER_IDLE_SEC").is_some());
        assert!(command_env(&steady, "SKWD_PAPER_TRANSITIONS").is_some());
        let ready_socket = wall_proto::resolve_socket().display().to_string();
        for command in [&steady, &transition, &scene] {
            assert_eq!(
                command_env(command, "SKWD_PAPER_READY_SOCKET").as_deref(),
                Some(ready_socket.as_str()),
            );
        }
        assert_eq!(command_env(&transition, "SKWD_PAPER_IDLE_SEC"), None);
        assert_eq!(command_env(&transition, "SKWD_PAPER_TRANSITIONS"), None);
        assert!(command_env(&scene, "SKWD_PAPER_WE_FPS").is_some());
        assert!(command_env(&scene, "SKWD_PAPER_OUTPUT_FPS").is_some());
    }

    #[test]
    fn failed_spawn_restores_incumbent() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing-renderer");
        let state = WallState::test_new(serde_json::json!({
            "paths": {"paperStillBin": missing.display().to_string()}
        }));
        let incumbent = sleeper();
        let incumbent_pid = incumbent.id();
        state.renderers().restore_base_still((incumbent, None));

        let result = RendererLaunchSpec::static_for("*", "/w/new.png", "fill").spawn(&state);

        assert!(result.is_err());
        assert_eq!(state.renderers().wallpaper_pids(), vec![incumbent_pid]);
        assert!(Path::new(&format!("/proc/{incumbent_pid}")).exists());
        state.renderers().kill_all();
    }

    #[test]
    fn cancellation_before_readiness_rolls_back_incumbent() {
        let directory = tempfile::tempdir().unwrap();
        let binary = directory.path().join("renderer");
        executable(&binary);
        let state = WallState::test_new(serde_json::json!({
            "paths": {"paperStillBin": binary.display().to_string()}
        }));
        let incumbent = sleeper();
        let incumbent_pid = incumbent.id();
        state.renderers().restore_base_still((incumbent, None));

        let startup =
            RendererLaunchSpec::static_for("*", "/w/new.png", "fill").spawn(&state).unwrap();
        let candidate_pid = startup.pid();
        drop(startup);

        assert!(!Path::new(&format!("/proc/{candidate_pid}")).exists());
        assert_eq!(state.renderers().wallpaper_pids(), vec![incumbent_pid]);
        assert!(Path::new(&format!("/proc/{incumbent_pid}")).exists());
        state.renderers().kill_all();
    }

    #[test]
    fn committed_output_replacement_retires_incumbent() {
        let directory = tempfile::tempdir().unwrap();
        let binary = directory.path().join("renderer");
        executable(&binary);
        let state = WallState::test_new(serde_json::json!({
            "paths": {"paperStillBin": binary.display().to_string()}
        }));
        let incumbent = sleeper();
        let incumbent_pid = incumbent.id();
        state.renderers().restore_base_still((incumbent, None));
        let startup =
            RendererLaunchSpec::static_for("*", "/w/new.png", "fill").spawn(&state).unwrap();
        let candidate_pid = startup.pid();
        state.renderers().signal_ready(candidate_pid);

        startup.wait_ready().unwrap().commit().unwrap();

        assert!(!Path::new(&format!("/proc/{incumbent_pid}")).exists());
        assert_eq!(state.renderers().wallpaper_pids(), vec![candidate_pid]);
        state.renderers().kill_all();
    }

    #[test]
    fn ready_signal_for_reaped_candidate_restores_incumbent() {
        let directory = tempfile::tempdir().unwrap();
        let binary = directory.path().join("renderer");
        std::fs::write(&binary, b"#!/bin/sh\nexit 0\n").unwrap();
        let mut permissions = std::fs::metadata(&binary).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
        std::fs::set_permissions(&binary, permissions).unwrap();
        let state = WallState::test_new(serde_json::json!({
            "paths": {"paperStillBin": binary.display().to_string()}
        }));
        let incumbent = sleeper();
        let incumbent_pid = incumbent.id();
        state.renderers().restore_base_still((incumbent, None));
        let startup =
            RendererLaunchSpec::static_for("*", "/w/new.png", "fill").spawn(&state).unwrap();
        let candidate_pid = startup.pid();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while state.renderers().wallpaper_pids().contains(&candidate_pid) {
            state.renderers().reap_exited();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        state.renderers().signal_ready(candidate_pid);

        assert!(startup.wait_ready().is_err());
        assert_eq!(state.renderers().wallpaper_pids(), vec![incumbent_pid]);
        assert!(Path::new(&format!("/proc/{incumbent_pid}")).exists());
        state.renderers().kill_all();
    }

    #[test]
    fn candidate_reaped_after_readiness_cannot_commit() {
        let directory = tempfile::tempdir().unwrap();
        let binary = directory.path().join("renderer");
        executable(&binary);
        let state = WallState::test_new(serde_json::json!({
            "paths": {"paperStillBin": binary.display().to_string()}
        }));
        let incumbent = sleeper();
        let incumbent_pid = incumbent.id();
        state.renderers().restore_base_still((incumbent, None));
        let startup =
            RendererLaunchSpec::static_for("*", "/w/new.png", "fill").spawn(&state).unwrap();
        let candidate_pid = startup.pid();
        state.renderers().signal_ready(candidate_pid);
        let ready = startup.wait_ready().unwrap();
        unsafe {
            libc::kill(candidate_pid.cast_signed(), libc::SIGKILL);
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while state.renderers().wallpaper_pids().contains(&candidate_pid) {
            state.renderers().reap_exited();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }

        assert!(ready.commit().is_err());
        assert_eq!(state.renderers().wallpaper_pids(), vec![incumbent_pid]);
        assert!(Path::new(&format!("/proc/{incumbent_pid}")).exists());
        state.renderers().kill_all();
    }

    #[test]
    fn native_scene_rollback_clears_role_marker() {
        let directory = tempfile::tempdir().unwrap();
        let binary = directory.path().join("renderer");
        executable(&binary);
        let state = WallState::test_new(serde_json::json!({
            "paths": {"paperVkBin": binary.display().to_string()}
        }));
        let startup = RendererLaunchSpec::native_scene(
            "DP-1",
            vec!["DP-1".into(), "/we/42".into(), "--scene".into(), "/we/42".into()],
        )
        .spawn(&state)
        .unwrap();
        let candidate_pid = startup.pid();
        assert!(state.renderers().is_scene_paper("DP-1"));

        drop(startup);

        assert!(!state.renderers().is_scene_paper("DP-1"));
        assert!(!Path::new(&format!("/proc/{candidate_pid}")).exists());
        assert!(!state.renderers().has_video_paper("DP-1"));
    }
}
