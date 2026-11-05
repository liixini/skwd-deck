use anyhow::Result;
use paper_control::{AudioSetRequest, PauseRequest, PlaybackControl};
use std::sync::atomic::Ordering;
use wall_proto::apply_fields::ALL_OUTPUTS;

use crate::infrastructure::paper::{PaperClient, Startup};
use crate::lock;

use super::RendererSupervisor;
use super::supervisor::PausePolicy;

impl PausePolicy {
    pub(super) fn paper_request(&self) -> PauseRequest {
        let outputs: std::collections::BTreeSet<_> =
            self.manual_outputs.keys().chain(&self.automatic_outputs).collect();
        PauseRequest {
            paused: self.paused(false),
            outputs: vec![ALL_OUTPUTS.to_string()],
            overrides: outputs
                .into_iter()
                .map(|output| (output.clone(), self.paused_for(false, output)))
                .collect(),
        }
    }
}

impl RendererSupervisor {
    pub fn sync_paper_controls(&self) -> Result<()> {
        let _controls = lock(&self.paper_controls);
        self.paper_pending.store(true, Ordering::Relaxed);
        if let Some(client) = PaperClient::existing()? {
            let playback = client.capabilities(Startup::ExistingOnly)?.controls.playback;
            let policy = lock(&self.pause).playback_policy();
            let request = if playback.contains(&PlaybackControl::ScopedPause) {
                policy.paper_request()
            } else {
                PauseRequest { paused: policy.paused(false), ..PauseRequest::default() }
            };
            let ducked = self.audio_ducked();
            let audio = playback
                .contains(&PlaybackControl::Duck)
                .then(|| AudioSetRequest { duck: Some(ducked), ..AudioSetRequest::default() });
            if ducked && let Some(audio) = &audio {
                client.audio(audio.clone())?;
            }
            client.set_paused(request)?;
            if !ducked && let Some(audio) = audio {
                client.audio(audio)?;
            }
            if let Some(scene) = self.placed_scene(None) {
                super::widgets::paper_widgets(&client, scene);
            }
        }
        self.paper_pending.store(false, Ordering::Relaxed);
        Ok(())
    }
}
