use std::path::Path;
use std::sync::Once;

use anyhow::{Context, anyhow};
use ffmpeg_the_third as ff;

static FFMPEG_INIT: Once = Once::new();

pub(super) fn init_ffmpeg() {
    FFMPEG_INIT.call_once(|| {
        let _ = ff::init();
        ff::log::set_level(ff::log::Level::Error);
    });
}

pub(super) struct VideoSource {
    pub(super) input: ff::format::context::Input,
    pub(super) stream_index: usize,
    pub(super) time_base_secs: f64,
}

impl VideoSource {
    pub(super) fn open(path: &Path) -> anyhow::Result<Self> {
        init_ffmpeg();
        let input = ff::format::input(path).with_context(|| format!("open {}", path.display()))?;
        let stream = input
            .streams()
            .best(ff::media::Type::Video)
            .ok_or_else(|| anyhow!("no video stream in {}", path.display()))?;
        let stream_index = stream.index();
        let time_base = stream.time_base();
        let time_base_secs =
            f64::from(time_base.numerator()) / f64::from(time_base.denominator()).max(1.0);
        Ok(Self { input, stream_index, time_base_secs })
    }

    pub(super) fn decoder(&self) -> anyhow::Result<ff::decoder::Video> {
        let context = self.codec_context()?;
        open_software_decoder(context)
    }

    pub(super) fn codec_context(&self) -> anyhow::Result<ff::codec::context::Context> {
        let stream =
            self.input.stream(self.stream_index).ok_or_else(|| anyhow!("video stream gone"))?;
        ensure_video_dimensions(&stream.parameters())?;
        Ok(ff::codec::context::Context::from_parameters(stream.parameters())?)
    }

    pub(super) fn duration_us(&self) -> i64 {
        self.input.duration().max(0)
    }

    pub(super) fn frame_rate(&self, fallback: u32) -> f64 {
        let Some(stream) = self.input.stream(self.stream_index) else {
            return f64::from(fallback);
        };
        let average = f64::from(stream.avg_frame_rate());
        if average.is_finite() && average > 0.0 {
            return average;
        }
        let nominal = f64::from(stream.rate());
        if nominal.is_finite() && nominal > 0.0 { nominal } else { f64::from(fallback) }
    }
}

pub(super) fn ensure_video_dimensions(
    parameters: &ff::codec::ParametersRef<'_>,
) -> anyhow::Result<()> {
    let (width, height) = (parameters.width(), parameters.height());
    anyhow::ensure!(
        crate::domain::media_limits::video_dimensions_allowed(width, height),
        "{width}x{height} exceeds the 4K-equivalent processing budget"
    );
    Ok(())
}

pub(super) fn open_software_decoder(
    context: ff::codec::context::Context,
) -> anyhow::Result<ff::decoder::Video> {
    let name = if context.id() == ff::codec::Id::AV1 { "libdav1d" } else { context.id().name() };
    let decoder = ff::codec::decoder::find_by_name(name)
        .ok_or_else(|| anyhow!("software {name} decoder missing"))?;
    context
        .decoder()
        .open_as(decoder)
        .with_context(|| format!("open software {name} decoder"))?
        .video()
        .with_context(|| format!("software {name} as video"))
}
