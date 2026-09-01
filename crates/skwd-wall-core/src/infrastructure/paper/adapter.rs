use anyhow::{Result, bail};

use crate::backend::wallpaper::ApplyOutputRequest;
use crate::config::Config;

use super::client::PaperClient;
use paper_control::{ApplyRequest, ApplyResult, Assignment, FillMode, Layer, Source, VideoEngine};

pub struct PaperClientAdapter {
    pub(super) client: PaperClient,
    pub(super) config: Config,
}

impl PaperClientAdapter {
    pub fn configured(config: &Config) -> Self {
        Self { client: PaperClient::configured(config), config: config.clone() }
    }

    pub fn new(client: PaperClient, config: Config) -> Self {
        Self { client, config }
    }

    pub fn apply(&self, assignments: Vec<Assignment>, replace_all: bool) -> Result<ApplyResult> {
        self.client.apply(ApplyRequest { assignments, replace_all, policy: None })
    }

    pub fn apply_request(&self, request: ApplyRequest) -> Result<ApplyResult> {
        self.client.apply(request)
    }

    pub fn apply_output(&self, request: ApplyOutputRequest<'_>) -> Result<ApplyResult> {
        self.apply(vec![self.assignment(request)?], false)
    }

    pub fn assignment(&self, request: ApplyOutputRequest<'_>) -> Result<Assignment> {
        let source = match request.kind {
            wall_proto::kind::STATIC => Source::static_file(request.path),
            wall_proto::kind::VIDEO => match video_engine(&self.config.renderer().video_engine()) {
                VideoEngine::Tinier => Source::tinier_video(
                    request.path,
                    request
                        .frame_rate
                        .ok_or_else(|| anyhow::anyhow!("tinier video has no frame rate"))?,
                ),
                VideoEngine::Default => Source::video(request.path, Some(VideoEngine::Default)),
            },
            wall_proto::kind::WE => {
                if !crate::we::valid_we_id(request.we_id) {
                    bail!("invalid Wallpaper Engine id: {}", request.we_id);
                }
                Source::wallpaper_engine(
                    self.config.we_dir().join(request.we_id).to_string_lossy().into_owned(),
                )
            }
            kind => bail!("unsupported Paper source kind {kind}"),
        };
        let mute = source.effective_video_engine() == Some(VideoEngine::Tinier) || request.mute;
        Ok(assignment_with_options(
            vec![request.output.to_string()],
            source,
            fill_mode(request.fill_mode),
            mute,
            request.volume,
            Layer::Background,
        ))
    }
}

pub(super) fn video_engine(value: &str) -> VideoEngine {
    match value {
        "tinier" => VideoEngine::Tinier,
        _ => VideoEngine::Default,
    }
}

pub(super) fn fill_mode(value: &str) -> FillMode {
    value.parse().unwrap_or_default()
}

pub(super) fn assignment_with_options(
    outputs: Vec<String>,
    source: Source,
    fill_mode: FillMode,
    mute: bool,
    volume: u32,
    layer: Layer,
) -> Assignment {
    Assignment {
        outputs,
        source,
        fill_mode,
        mute,
        volume: volume.min(100),
        layer,
        transition: None,
    }
}
