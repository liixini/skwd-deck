mod drawing;
mod handoff;
pub use drawing::preview;

use std::sync::Arc;

use crate::backend::wallpaper::{
    ApplyOutputRequest, ApplyStaticRequest, ApplyVideoRequest, StaticSmartRequest,
    VideoTransitionRequest, WallpaperApplication,
};
use crate::state::WallState;

pub struct CoreWallpaperApplication {
    state: Arc<WallState>,
}

impl CoreWallpaperApplication {
    pub fn new(state: Arc<WallState>) -> Self {
        Self { state }
    }

    fn note_prepared(&self, started: std::time::Instant) {
        self.state
            .apply()
            .note_phase(crate::backend::wallpaper::ApplyPhase::Prepare, started.elapsed());
    }

    fn retire_paper<T>(&self, output: &str, result: anyhow::Result<T>) -> anyhow::Result<T> {
        let applied = result?;
        let socket = crate::infrastructure::paper::paper_socket_path();
        if socket.exists() {
            let config = self.state.config();
            let outputs = crate::outputs::enumerate()
                .into_iter()
                .filter(|candidate| {
                    (output == wall_proto::apply_fields::ALL_OUTPUTS || output == candidate.name)
                        && !config.display().output_locked(&candidate.name)
                })
                .map(|candidate| candidate.name)
                .collect::<Vec<_>>();
            if outputs.is_empty() {
                return Ok(applied);
            }
            crate::infrastructure::paper::PaperClient::new(config.renderer().paper_bin(), socket)
                .stop(outputs)?;
        }
        Ok(applied)
    }
}

impl WallpaperApplication for CoreWallpaperApplication {
    fn release_outputs(&self, outputs: &[String]) -> anyhow::Result<()> {
        anyhow::ensure!(!outputs.is_empty(), "external handoff requires at least one output");
        let socket = crate::infrastructure::paper::paper_socket_path();
        if socket.exists() {
            crate::infrastructure::paper::PaperClient::new(
                self.state.config().renderer().paper_bin(),
                socket,
            )
            .stop(outputs.to_vec())?;
        }
        crate::apply::release_outputs(&self.state, outputs)
    }

    fn apply_static(&self, request: ApplyStaticRequest<'_>) -> anyhow::Result<()> {
        let started = std::time::Instant::now();
        if drawing::apply(
            &self.state,
            std::path::Path::new(request.path),
            request.output,
            request.targets,
            request.previous_static,
        )? {
            return Ok(());
        }
        if handoff::image(
            &self.state,
            StaticSmartRequest {
                output: request.output,
                path: request.path,
                fill_mode: request.fill_mode,
            },
            crate::backend::wallpaper::OutputTransitionRequest {
                enabled: request.transition,
                shader: request.shader,
                duration_ms: request.duration_ms,
            },
            request.targets,
        )? {
            return Ok(());
        }
        self.note_prepared(started);
        self.retire_paper(
            request.output,
            crate::apply::apply_static(
                &self.state,
                request.output,
                request.path,
                request.fill_mode,
                request.from,
                request.transition,
                request.shader,
                request.duration_ms,
            ),
        )
    }

    fn apply_static_smart(&self, request: StaticSmartRequest<'_>) -> anyhow::Result<()> {
        if drawing::apply(
            &self.state,
            std::path::Path::new(request.path),
            request.output,
            &[],
            None,
        )? {
            return Ok(());
        }
        if handoff::image(
            &self.state,
            request,
            crate::backend::wallpaper::OutputTransitionRequest {
                enabled: false,
                shader: "",
                duration_ms: 0,
            },
            &[],
        )? {
            return Ok(());
        }
        self.retire_paper(
            request.output,
            crate::apply::apply_static_smart(
                &self.state,
                request.output,
                request.path,
                request.fill_mode,
            ),
        )
    }

    fn apply_video(&self, request: ApplyVideoRequest<'_>) -> anyhow::Result<()> {
        if handoff::apply(
            &self.state,
            ApplyOutputRequest {
                output: request.output,
                kind: wall_proto::kind::VIDEO,
                path: request.path,
                we_id: "",
                fill_mode: request.fill_mode,
                mute: request.mute,
                volume: request.volume,
                transition: None,
                fanout: None,
            },
            &[],
        )? {
            return Ok(());
        }
        if crate::apply::independent_playback(&self.state) {
            return self.retire_paper(
                request.output,
                crate::apply::apply_independent_video(&self.state, request, None),
            );
        }
        self.retire_paper(
            request.output,
            crate::apply::apply_video(
                &self.state,
                request.output,
                request.path,
                request.fill_mode,
                request.mute,
                request.volume,
            ),
        )
    }

    fn apply_output(&self, request: ApplyOutputRequest<'_>) -> anyhow::Result<()> {
        let started = std::time::Instant::now();
        if request.kind == wall_proto::kind::STATIC
            && drawing::apply(
                &self.state,
                std::path::Path::new(request.path),
                request.output,
                &[],
                None,
            )?
        {
            return Ok(());
        }
        let targets = request.fanout.map_or(&[][..], |fanout| fanout.targets);
        if handoff::apply(&self.state, request, targets)? {
            return Ok(());
        }
        self.note_prepared(started);
        self.retire_paper(
            request.output,
            crate::apply::apply_output_with_transition(
                &self.state,
                request.output,
                request.kind,
                request.path,
                request.we_id,
                request.fill_mode,
                request.mute,
                request.volume,
                request.transition,
            ),
        )
    }

    fn apply_video_transition(&self, request: VideoTransitionRequest<'_>) -> anyhow::Result<()> {
        if handoff::apply(
            &self.state,
            ApplyOutputRequest {
                output: wall_proto::apply_fields::ALL_OUTPUTS,
                kind: wall_proto::kind::VIDEO,
                path: request.to,
                we_id: "",
                fill_mode: request.fill_mode,
                mute: request.mute,
                volume: request.volume,
                transition: Some(crate::backend::wallpaper::OutputTransitionRequest {
                    enabled: true,
                    shader: request.shader,
                    duration_ms: request.duration_ms,
                }),
                fanout: None,
            },
            &[],
        )? {
            return Ok(());
        }
        if crate::apply::independent_playback(&self.state) {
            return self.retire_paper(
                wall_proto::apply_fields::ALL_OUTPUTS,
                crate::apply::apply_independent_video(
                    &self.state,
                    ApplyVideoRequest {
                        output: "*",
                        path: request.to,
                        fill_mode: request.fill_mode,
                        mute: request.mute,
                        volume: request.volume,
                    },
                    Some(crate::backend::wallpaper::OutputTransitionRequest {
                        enabled: true,
                        shader: request.shader,
                        duration_ms: request.duration_ms,
                    }),
                ),
            );
        }
        self.retire_paper(
            wall_proto::apply_fields::ALL_OUTPUTS,
            crate::apply::apply_video_transition(
                &self.state,
                request.from,
                request.to,
                request.fill_mode,
                request.shader,
                request.duration_ms,
                request.mute,
                request.volume,
            ),
        )
    }

    fn apply_we(
        &self,
        we_id: &str,
        transition: Option<crate::backend::wallpaper::OutputTransitionRequest<'_>>,
    ) -> anyhow::Result<Option<String>> {
        let started = std::time::Instant::now();
        let item = crate::we::require_project(&self.state, we_id)?;
        let config = self.state.config();
        let (mute, volume) = crate::audio::resolve_defaults(
            &config.cache_dir(),
            wall_proto::apply_fields::ALL_OUTPUTS,
            config.renderer().mute(),
            config.renderer().volume(),
        );
        if handoff::apply(
            &self.state,
            ApplyOutputRequest {
                output: wall_proto::apply_fields::ALL_OUTPUTS,
                kind: wall_proto::kind::WE,
                path: "",
                we_id,
                fill_mode: &config.display().fill_mode(),
                mute,
                volume,
                transition,
                fanout: None,
            },
            &[],
        )? {
            return Ok(crate::we::find_preview(&item).map(|path| path.display().to_string()));
        }
        self.note_prepared(started);
        self.retire_paper(
            wall_proto::apply_fields::ALL_OUTPUTS,
            crate::we::apply_we(&self.state, we_id, transition),
        )
    }

    fn reload_we(&self) -> anyhow::Result<()> {
        crate::plasma::require_backend()?;
        self.retire_paper(
            wall_proto::apply_fields::ALL_OUTPUTS,
            crate::apply::reload_we(&self.state),
        )
    }
}

#[cfg(test)]
fn stationary_child(test: &str) -> bool {
    if std::env::var_os("NIRI_SOCKET").is_some() {
        return false;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture"])
        .env("NIRI_SOCKET", "fixture")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success() && text.contains("1 passed"), "{text}");
    true
}
