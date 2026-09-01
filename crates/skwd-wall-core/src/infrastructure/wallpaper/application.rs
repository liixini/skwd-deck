use std::sync::Arc;

use anyhow::bail;

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

    fn stop_paper(&self) -> anyhow::Result<()> {
        let socket = crate::infrastructure::paper::paper_socket_path();
        if socket.exists() {
            crate::infrastructure::paper::PaperClient::new(
                self.state.config().renderer().paper_bin(),
                socket,
            )
            .stop(Vec::new())?;
        }
        Ok(())
    }

    fn apply_tinier(&self, request: ApplyOutputRequest<'_>) -> anyhow::Result<()> {
        let outputs = crate::outputs::enumerate();
        if outputs.is_empty() {
            bail!("Tinier requires at least one named output");
        }
        if !outputs.iter().any(|output| output.name == request.output) {
            bail!("Tinier output {} is not live", request.output);
        }
        if crate::plasma::available() {
            self.stop_paper()?;
            return crate::apply::apply_output_with_transition(
                &self.state,
                request.output,
                request.kind,
                request.path,
                request.we_id,
                request.fill_mode,
                request.mute,
                request.volume,
                request.transition,
            );
        }
        let cache = self.state.config().cache_dir();
        let mut candidate = crate::audio::read_state(&cache);
        let map = candidate.as_object_mut().expect("audio state is always an object");
        if let Some(wildcard) = map.remove("*") {
            for output in &outputs {
                map.entry(output.name.clone()).or_insert_with(|| wildcard.clone());
            }
        }
        map.insert(
            request.output.to_string(),
            crate::audio::entry(wall_proto::kind::VIDEO, request.path, "", true, request.volume),
        );
        let adapter =
            crate::infrastructure::paper::PaperClientAdapter::configured(&self.state.config());
        let plan = adapter.tinier_composition_plan(&self.state, &candidate, &outputs)?;
        let crate::infrastructure::paper::PaperCompositionPlan::Replace(apply) = plan else {
            bail!("Tinier composition has no live assignments");
        };
        adapter.apply_request(apply)?;
        crate::audio::write_state(&cache, &candidate);
        crate::awww::stop();
        self.state.renderers().kill_all();
        let assignments = outputs
            .into_iter()
            .filter_map(|output| {
                let entry = candidate.get(&output.name)?;
                let path = entry.get("path")?.as_str()?;
                Some((output.name, path.to_string()))
            })
            .collect();
        self.state.renderers().replace_assignments(assignments);
        Ok(())
    }
}

impl WallpaperApplication for CoreWallpaperApplication {
    fn apply_static(&self, request: ApplyStaticRequest<'_>) -> anyhow::Result<()> {
        self.stop_paper()?;
        crate::apply::apply_static(
            &self.state,
            request.output,
            request.path,
            request.fill_mode,
            request.from,
            request.transition,
            request.shader,
            request.duration_ms,
        )
    }

    fn apply_static_smart(&self, request: StaticSmartRequest<'_>) -> anyhow::Result<()> {
        self.stop_paper()?;
        crate::apply::apply_static_smart(
            &self.state,
            request.output,
            request.path,
            request.fill_mode,
        )
    }

    fn apply_video(&self, request: ApplyVideoRequest<'_>) -> anyhow::Result<()> {
        self.stop_paper()?;
        crate::apply::apply_video(
            &self.state,
            request.output,
            request.path,
            request.fill_mode,
            request.mute,
            request.volume,
        )
    }

    fn apply_output(&self, request: ApplyOutputRequest<'_>) -> anyhow::Result<()> {
        if request.kind == wall_proto::kind::VIDEO
            && self.state.config().renderer().video_engine() == "tinier"
            && request.frame_rate.is_some()
        {
            return self.apply_tinier(request);
        }
        self.stop_paper()?;
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
        )
    }

    fn apply_video_transition(&self, request: VideoTransitionRequest<'_>) -> anyhow::Result<()> {
        self.stop_paper()?;
        crate::apply::apply_video_transition(
            &self.state,
            request.from,
            request.to,
            request.fill_mode,
            request.shader,
            request.duration_ms,
            request.mute,
            request.volume,
        )
    }

    fn apply_we(&self, we_id: &str) -> anyhow::Result<Option<String>> {
        self.stop_paper()?;
        crate::we::apply_we(&self.state, we_id)
    }

    fn video_engine_is_vk(&self) -> bool {
        crate::apply::video_engine_is_vk(&self.state)
    }

    fn reload_we(&self) -> anyhow::Result<()> {
        self.stop_paper()?;
        crate::apply::reload_we(&self.state)
    }
}
