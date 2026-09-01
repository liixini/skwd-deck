use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicI32, Ordering};

use ffmpeg_the_third as ff;

use super::preview_policy::FrameGate;
use super::scaling::CoverScaler;
use super::source::{VideoSource, open_software_decoder};

pub(super) const DEFAULT_LIVE_PREVIEW_FPS: u32 = 30;
const LIVE_PREVIEW_DECODER_ENV: &str = "SKWD_LIVE_PREVIEW_DECODER";
const LIVE_PREVIEW_THREADS_ENV: &str = "SKWD_LIVE_PREVIEW_THREADS";
static HARDWARE_PIXEL_FORMAT: AtomicI32 = AtomicI32::new(-1);
static VIRTUAL_DRM: OnceLock<bool> = OnceLock::new();

unsafe extern "C" fn get_hardware_format(
    _context: *mut ff::ffi::AVCodecContext,
    mut formats: *const ff::ffi::AVPixelFormat,
) -> ff::ffi::AVPixelFormat {
    let wanted = HARDWARE_PIXEL_FORMAT.load(Ordering::Relaxed);
    unsafe {
        while (*formats).0 != -1 {
            if (*formats).0 == wanted {
                return *formats;
            }
            formats = formats.add(1);
        }
    }
    ff::ffi::AVPixelFormat(-1)
}

pub(super) struct HardwareDecoder {
    device_context: *mut ff::ffi::AVBufferRef,
    pixel_format: ff::format::Pixel,
}

impl HardwareDecoder {
    unsafe fn attach(&self, context: &mut ff::codec::context::Context) {
        unsafe {
            let raw = context.as_mut_ptr();
            (*raw).hw_device_ctx = ff::ffi::av_buffer_ref(self.device_context);
            (*raw).get_format = Some(get_hardware_format);
        }
    }

    unsafe fn open(context: &mut ff::codec::context::Context) -> Option<(Self, ff::format::Pixel)> {
        use ff::ffi;

        if virtual_drm_detected() {
            return None;
        }
        unsafe {
            let raw = context.as_mut_ptr();
            let codec = ffi::avcodec_find_decoder((*raw).codec_id);
            if codec.is_null() {
                return None;
            }
            let mut index = 0;
            loop {
                let configuration = ffi::avcodec_get_hw_config(codec, index);
                if configuration.is_null() {
                    return None;
                }
                index += 1;
                let configuration = &*configuration;
                if (configuration.methods & ffi::AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX.0 as i32)
                    == 0
                {
                    continue;
                }
                let mut device_context = std::ptr::null_mut();
                let result = ffi::av_hwdevice_ctx_create(
                    &mut device_context,
                    configuration.device_type,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    0,
                );
                if result < 0 || device_context.is_null() {
                    continue;
                }
                HARDWARE_PIXEL_FORMAT.store(configuration.pix_fmt.0, Ordering::Relaxed);
                (*raw).hw_device_ctx = ffi::av_buffer_ref(device_context);
                (*raw).get_format = Some(get_hardware_format);
                let pixel_format = ff::format::Pixel::from(configuration.pix_fmt);
                return Some((Self { device_context, pixel_format }, pixel_format));
            }
        }
    }

    fn pixel_format(&self) -> ff::format::Pixel {
        self.pixel_format
    }
}

impl Drop for HardwareDecoder {
    fn drop(&mut self) {
        unsafe { ff::ffi::av_buffer_unref(&mut self.device_context) };
    }
}

fn virtual_drm_detected() -> bool {
    *VIRTUAL_DRM.get_or_init(|| {
        std::fs::read_dir("/sys/class/drm")
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with("card"))
            .filter_map(|entry| std::fs::read_to_string(entry.path().join("device/vendor")).ok())
            .any(|vendor| is_virtio_vendor(&vendor))
    })
}

pub(crate) fn is_virtio_vendor(vendor: &str) -> bool {
    vendor.trim().eq_ignore_ascii_case("0x1af4")
}

pub(super) struct FramePipeline {
    software_frame: ff::frame::Video,
    scaler: CoverScaler,
    rgba: Vec<u8>,
    pacer: PreviewPacer,
    #[cfg(test)]
    observed_backend: Option<DecodeBackend>,
    #[cfg(test)]
    backend_probe: Option<std::sync::Arc<std::sync::atomic::AtomicUsize>>,
    #[cfg(test)]
    release_probe: Option<ReleaseProbe>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DecodeBackend {
    Software,
    HardwareTransfer,
}

#[derive(Debug)]
pub(super) enum FrameFailure {
    DecoderReceive(ff::Error),
    HardwareTransfer(i32),
    Scale(String),
}

pub(super) enum FrameOutcome<'a> {
    Frame { rgba: &'a [u8], backend: DecodeBackend },
    Skipped,
    NeedPacket,
    EndOfFile,
    Failed(FrameFailure),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReceiveOutcome {
    NeedPacket,
    EndOfFile,
    Failed(ff::Error),
}

impl FramePipeline {
    pub(super) fn new(width: u32, height: u32, fps_cap: u32, source_fps: f64) -> Self {
        Self {
            software_frame: ff::frame::Video::empty(),
            scaler: CoverScaler::new(width, height, ff::format::Pixel::RGBA, 4),
            rgba: Vec::new(),
            pacer: PreviewPacer::new(fps_cap, source_fps),
            #[cfg(test)]
            observed_backend: None,
            #[cfg(test)]
            backend_probe: None,
            #[cfg(test)]
            release_probe: None,
        }
    }

    pub(super) fn next<'a>(
        &'a mut self,
        decoder: &mut ff::decoder::Video,
        hardware_format: Option<ff::format::Pixel>,
        time_base_seconds: f64,
    ) -> FrameOutcome<'a> {
        let mut frame = ff::frame::Video::empty();
        match decoder.receive_frame(&mut frame) {
            Ok(()) => {}
            Err(error) => match classify_receive_error(error) {
                ReceiveOutcome::NeedPacket => return FrameOutcome::NeedPacket,
                ReceiveOutcome::EndOfFile => return FrameOutcome::EndOfFile,
                ReceiveOutcome::Failed(error) => {
                    return FrameOutcome::Failed(FrameFailure::DecoderReceive(error));
                }
            },
        }
        if !self.pacer.admit(frame.timestamp().or_else(|| frame.pts()), time_base_seconds) {
            return FrameOutcome::Skipped;
        }
        let hardware = hardware_format.is_some_and(|format| frame.format() == format);
        if hardware && let Err(error) = transfer_hardware(&frame, &mut self.software_frame) {
            return FrameOutcome::Failed(FrameFailure::HardwareTransfer(error));
        }
        let source = if hardware { &self.software_frame } else { &frame };
        if let Err(error) = self.scaler.cover_into(source, &mut self.rgba) {
            return FrameOutcome::Failed(FrameFailure::Scale(error.to_string()));
        }
        let backend =
            if hardware { DecodeBackend::HardwareTransfer } else { DecodeBackend::Software };
        #[cfg(test)]
        {
            self.observed_backend = Some(backend);
        }
        #[cfg(test)]
        if let Some(probe) = &self.backend_probe {
            let value = match backend {
                DecodeBackend::Software => 1,
                DecodeBackend::HardwareTransfer => 2,
            };
            probe.store(value, Ordering::SeqCst);
        }
        FrameOutcome::Frame { rgba: &self.rgba, backend }
    }

    pub(super) fn rewind(&mut self, fps_cap: u32, source_fps: f64) {
        self.pacer = PreviewPacer::new(fps_cap, source_fps);
    }

    #[cfg(test)]
    pub(super) fn observed_backend(&self) -> Option<DecodeBackend> {
        self.observed_backend
    }

    #[cfg(test)]
    pub(super) fn with_release_probe(
        mut self,
        probe: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) -> Self {
        self.release_probe = Some(ReleaseProbe(probe));
        self
    }

    #[cfg(test)]
    pub(super) fn with_backend_probe(
        mut self,
        probe: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) -> Self {
        self.backend_probe = Some(probe);
        self
    }
}

fn classify_receive_error(error: ff::Error) -> ReceiveOutcome {
    match error {
        ff::Error::Eof => ReceiveOutcome::EndOfFile,
        ff::Error::Other { errno } if errno == libc::EAGAIN => ReceiveOutcome::NeedPacket,
        error => ReceiveOutcome::Failed(error),
    }
}

#[cfg(test)]
struct ReleaseProbe(std::sync::Arc<std::sync::atomic::AtomicUsize>);

#[cfg(test)]
impl Drop for ReleaseProbe {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DecodePassEnd {
    EndOfFile { frames: usize },
    OutputClosed,
}

pub fn stream_video_frames(
    source_path: &Path,
    width: u32,
    height: u32,
    output: &mut impl std::io::Write,
) -> anyhow::Result<()> {
    loop {
        let mut source = VideoSource::open(source_path)?;
        let source_fps = source.frame_rate(DEFAULT_LIVE_PREVIEW_FPS);
        let (mut decoder, _hardware, hardware_format) =
            open_fast_decoder(&source, source_fps, DEFAULT_LIVE_PREVIEW_FPS)?;
        let mut frames = FramePipeline::new(width, height, DEFAULT_LIVE_PREVIEW_FPS, source_fps);
        match decode_pass(&mut source, &mut decoder, hardware_format, &mut frames, output)? {
            DecodePassEnd::EndOfFile { frames: 0 } | DecodePassEnd::OutputClosed => return Ok(()),
            DecodePassEnd::EndOfFile { .. } => {}
        }
    }
}

fn decode_pass(
    source: &mut VideoSource,
    decoder: &mut ff::decoder::Video,
    hardware_format: Option<ff::format::Pixel>,
    frames: &mut FramePipeline,
    output: &mut impl std::io::Write,
) -> anyhow::Result<DecodePassEnd> {
    let stream_index = source.stream_index;
    let time_base_seconds = source.time_base_secs;
    let mut emitted = 0usize;
    for result in source.input.packets() {
        let (stream, packet) = result.map_err(|error| anyhow::anyhow!("video demux: {error}"))?;
        if stream.index() != stream_index {
            continue;
        }
        decoder.send_packet(&packet).map_err(|error| anyhow::anyhow!("video packet: {error}"))?;
        match drain_frames(
            decoder,
            hardware_format,
            time_base_seconds,
            frames,
            output,
            &mut emitted,
        )? {
            DrainEnd::NeedPacket => {}
            DrainEnd::EndOfFile => return Ok(DecodePassEnd::EndOfFile { frames: emitted }),
            DrainEnd::OutputClosed => return Ok(DecodePassEnd::OutputClosed),
        }
    }
    decoder.send_eof().map_err(|error| anyhow::anyhow!("video drain start: {error}"))?;
    match drain_frames(decoder, hardware_format, time_base_seconds, frames, output, &mut emitted)? {
        DrainEnd::EndOfFile => Ok(DecodePassEnd::EndOfFile { frames: emitted }),
        DrainEnd::OutputClosed => Ok(DecodePassEnd::OutputClosed),
        DrainEnd::NeedPacket => anyhow::bail!("video decoder requested a packet while draining"),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DrainEnd {
    NeedPacket,
    EndOfFile,
    OutputClosed,
}

fn drain_frames(
    decoder: &mut ff::decoder::Video,
    hardware_format: Option<ff::format::Pixel>,
    time_base_seconds: f64,
    frames: &mut FramePipeline,
    output: &mut impl std::io::Write,
    emitted: &mut usize,
) -> anyhow::Result<DrainEnd> {
    loop {
        match frames.next(decoder, hardware_format, time_base_seconds) {
            FrameOutcome::Frame { rgba, backend } => {
                if *emitted == 0 {
                    log::info!("stream: observed frame backend {backend:?}");
                }
                if output.write_all(rgba).and_then(|()| output.flush()).is_err() {
                    return Ok(DrainEnd::OutputClosed);
                }
                *emitted += 1;
            }
            FrameOutcome::Skipped => {}
            FrameOutcome::NeedPacket => return Ok(DrainEnd::NeedPacket),
            FrameOutcome::EndOfFile => return Ok(DrainEnd::EndOfFile),
            FrameOutcome::Failed(error) => anyhow::bail!("video frame: {error:?}"),
        }
    }
}

fn open_fast_decoder(
    source: &VideoSource,
    source_fps: f64,
    fps_cap: u32,
) -> anyhow::Result<(ff::decoder::Video, Option<HardwareDecoder>, Option<ff::format::Pixel>)> {
    let mut context = source.codec_context()?;
    let thread_cap = if fps_cap <= 20 { 2 } else { 4 };
    let count = std::thread::available_parallelism()
        .map_or(thread_cap, |available| available.get().clamp(2, thread_cap));
    configure_fast_decoder(&mut context, count);
    let (hardware, format) = unsafe { HardwareDecoder::open(&mut context) }
        .map_or((None, None), |(decoder, format)| (Some(decoder), Some(format)));
    if let Some(format) = format {
        log::info!("stream: hw decode configured ({format:?})");
    } else {
        log::info!("stream: software decode");
    }
    let decoder = if hardware.is_some() {
        context.decoder().video()?
    } else {
        open_software_decoder(context)?
    };
    log::info!("stream: source={source_fps:.2}fps cap={fps_cap}fps threads={count}");
    Ok((decoder, hardware, format))
}

pub(super) fn open_persistent_decoder(
    source: &VideoSource,
    source_fps: f64,
    fps_cap: u32,
    hardware: &mut Option<HardwareDecoder>,
) -> Option<(ff::decoder::Video, Option<ff::format::Pixel>)> {
    let mut context = source.codec_context().ok()?;
    let thread_cap = if fps_cap <= 20 { 2 } else { 4 };
    let default_threads = std::thread::available_parallelism()
        .map_or(thread_cap, |available| available.get().clamp(2, thread_cap));
    let count = std::env::var(LIVE_PREVIEW_THREADS_ENV)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .map_or(default_threads, |threads| threads.clamp(1, 8));
    configure_fast_decoder(&mut context, count);
    let software_only = std::env::var(LIVE_PREVIEW_DECODER_ENV)
        .is_ok_and(|value| value.eq_ignore_ascii_case("software"));
    let hardware_format = if software_only {
        log::info!("stream-persist: software decode requested");
        None
    } else if let Some(decoder) = hardware.as_ref() {
        unsafe { decoder.attach(&mut context) };
        Some(decoder.pixel_format())
    } else if let Some((decoder, format)) = unsafe { HardwareDecoder::open(&mut context) } {
        log::info!("stream-persist: hw decode configured ({format:?})");
        *hardware = Some(decoder);
        Some(format)
    } else {
        log::info!("stream-persist: software decode");
        None
    };
    let decoder = if hardware_format.is_some() {
        context.decoder().video().ok()?
    } else {
        open_software_decoder(context).ok()?
    };
    log::info!("stream-persist: source={source_fps:.2}fps cap={fps_cap}fps threads={count}");
    Some((decoder, hardware_format))
}

fn configure_fast_decoder(context: &mut ff::codec::context::Context, thread_count: usize) {
    context.set_threading(ff::codec::threading::Config {
        kind: ff::codec::threading::Type::Frame,
        count: thread_count,
    });
    unsafe { (*context.as_mut_ptr()).flags2 |= ff::ffi::AV_CODEC_FLAG2_FAST };
}

fn transfer_hardware(
    frame: &ff::frame::Video,
    software_frame: &mut ff::frame::Video,
) -> Result<(), i32> {
    let result = unsafe {
        ff::ffi::av_hwframe_transfer_data(software_frame.as_mut_ptr(), frame.as_ptr(), 0)
    };
    if result >= 0 { Ok(()) } else { Err(result) }
}

struct MediaClock {
    frame_step: f64,
    next_fallback: f64,
    last_output: Option<f64>,
}

impl MediaClock {
    fn new(source_fps: f64) -> Self {
        Self {
            frame_step: 1.0 / source_fps.clamp(1.0, 1000.0),
            next_fallback: 0.0,
            last_output: None,
        }
    }

    fn seconds(&mut self, pts: Option<i64>, time_base: f64) -> f64 {
        let candidate = pts.map(|value| value as f64 * time_base).filter(|value| value.is_finite());
        if let Some(value) = candidate
            && self.last_output.is_none_or(|last| value > last + 1e-9)
        {
            self.last_output = Some(value);
            self.next_fallback = value + self.frame_step;
            return value;
        }
        let value = self.next_fallback;
        self.next_fallback += self.frame_step;
        self.last_output = Some(value);
        value
    }
}

struct PreviewPacer {
    clock: MediaClock,
    gate: FrameGate,
    last_output: Option<f64>,
}

impl PreviewPacer {
    fn new(fps_cap: u32, source_fps: f64) -> Self {
        Self {
            clock: MediaClock::new(source_fps),
            gate: FrameGate::new(fps_cap),
            last_output: None,
        }
    }

    fn admit(&mut self, pts: Option<i64>, time_base: f64) -> bool {
        let seconds = self.clock.seconds(pts, time_base);
        if !self.gate.keep(seconds) {
            return false;
        }
        pace(&mut self.last_output, seconds);
        true
    }
}

fn pace(last_pts: &mut Option<f64>, frame_seconds: f64) {
    if let Some(previous) = *last_pts {
        let delay = (frame_seconds - previous).clamp(0.0, 0.2);
        if delay > 0.0 {
            std::thread::sleep(std::time::Duration::from_secs_f64(delay));
        }
    }
    *last_pts = Some(frame_seconds);
}

#[cfg(test)]
mod tests;
