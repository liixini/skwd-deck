use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ffmpeg_the_third as ff;

use crate::lock;

use super::cancellation::{Cancellation, CancellationState, WakeReason};
use super::decoding::{
    DEFAULT_LIVE_PREVIEW_FPS, FrameFailure, FrameOutcome, FramePipeline, HardwareDecoder,
    open_persistent_decoder,
};
use super::source::VideoSource;

const IDLE_EXIT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, Eq, PartialEq)]
struct StreamRequest {
    path: String,
    token: u32,
    fps: u32,
}

struct PersistState {
    request: std::sync::Mutex<Option<StreamRequest>>,
    cancellation: Cancellation,
}

impl PersistState {
    fn new() -> Self {
        Self { request: std::sync::Mutex::new(None), cancellation: Cancellation::new() }
    }

    fn replace(&self, request: Option<StreamRequest>) {
        *lock(&self.request) = request;
        self.cancellation.replace();
    }

    fn close(&self) {
        self.cancellation.close();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlaybackEnd {
    EndOfFile { frames: usize },
    Replaced,
    Cancelled,
}

#[derive(Debug)]
enum PlaybackFailure {
    Demux(ff::Error),
    PacketSend(ff::Error),
    DrainStart(ff::Error),
    DrainIncomplete,
    DecoderReceive(ff::Error),
    HardwareTransfer(i32),
    Scale(String),
    Output(std::io::Error),
}

#[derive(Debug)]
enum SourceEnd {
    Replaced,
    Cancelled,
    Unavailable,
    Failed(PlaybackFailure),
}

pub fn stream_video_frames_persist(
    width: u32,
    height: u32,
    output: &mut impl std::io::Write,
) -> anyhow::Result<()> {
    let state = Arc::new(PersistState::new());
    spawn_request_reader(Arc::clone(&state));
    let mut hardware = None;
    let mut idle_deadline = None;
    while !state.cancellation.closed() {
        let generation = state.cancellation.snapshot();
        let request = lock(&state.request).clone();
        if let Some(request) = request {
            idle_deadline = None;
            let end = stream_source(
                &request,
                width,
                height,
                output,
                &mut hardware,
                &state.cancellation,
                generation,
            );
            match end {
                SourceEnd::Replaced => {}
                SourceEnd::Cancelled => break,
                SourceEnd::Unavailable => {
                    let _ = state.cancellation.wait_until(generation, Instant::now() + IDLE_EXIT);
                }
                SourceEnd::Failed(error) => anyhow::bail!("persistent video stream: {error}"),
            }
        } else {
            let deadline = *idle_deadline.get_or_insert_with(|| Instant::now() + IDLE_EXIT);
            if state.cancellation.wait_until(generation, deadline) == WakeReason::Deadline {
                break;
            }
        }
    }
    Ok(())
}

fn spawn_request_reader(state: Arc<PersistState>) {
    std::thread::spawn(move || {
        read_requests(std::io::stdin().lock(), &state);
        state.close();
    });
}

fn read_requests(input: impl std::io::BufRead, state: &PersistState) {
    for line in input.lines() {
        let Ok(line) = line else { break };
        state.replace(parse_request(line.trim()));
    }
}

fn parse_request(text: &str) -> Option<StreamRequest> {
    let default_fps = std::env::var("SKWD_LIVE_PREVIEW_FPS")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(DEFAULT_LIVE_PREVIEW_FPS)
        .clamp(1, 60);
    parse_request_with_default(text, default_fps)
}

fn parse_request_with_default(text: &str, default_fps: u32) -> Option<StreamRequest> {
    if text.is_empty() {
        return None;
    }
    let mut fields = text.splitn(3, '\t');
    let path = fields.next().unwrap_or_default().to_string();
    let token = fields.next().and_then(|value| value.parse::<u32>().ok()).unwrap_or(0);
    let fps = fields
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(default_fps)
        .clamp(1, 60);
    Some(StreamRequest { path, token, fps })
}

fn stream_source(
    request: &StreamRequest,
    width: u32,
    height: u32,
    output: &mut impl std::io::Write,
    hardware: &mut Option<HardwareDecoder>,
    cancellation: &Cancellation,
    generation: u64,
) -> SourceEnd {
    stream_source_with(
        request,
        width,
        height,
        output,
        hardware,
        cancellation,
        generation,
        |w, h| FramePipeline::new(w, h, request.fps, f64::from(DEFAULT_LIVE_PREVIEW_FPS)),
    )
}

fn stream_source_with<F>(
    request: &StreamRequest,
    width: u32,
    height: u32,
    output: &mut impl std::io::Write,
    hardware: &mut Option<HardwareDecoder>,
    cancellation: &Cancellation,
    generation: u64,
    make_frames: F,
) -> SourceEnd
where
    F: FnOnce(u32, u32) -> FramePipeline,
{
    match cancellation.state(generation) {
        CancellationState::Active => {}
        CancellationState::Replaced => return SourceEnd::Replaced,
        CancellationState::Closed => return SourceEnd::Cancelled,
    }
    let Ok(mut source) = VideoSource::open(Path::new(&request.path)) else {
        return SourceEnd::Unavailable;
    };
    let source_fps = source.frame_rate(DEFAULT_LIVE_PREVIEW_FPS);
    let Some((mut decoder, hardware_format)) =
        open_persistent_decoder(&source, source_fps, request.fps, hardware)
    else {
        return SourceEnd::Unavailable;
    };
    let mut frames = make_frames(width, height);
    frames.rewind(request.fps, source_fps);
    loop {
        match play_once(
            &mut source,
            &mut decoder,
            hardware_format,
            &mut frames,
            request.token,
            output,
            cancellation,
            generation,
        ) {
            Ok(PlaybackEnd::Replaced) => return SourceEnd::Replaced,
            Ok(PlaybackEnd::Cancelled) => return SourceEnd::Cancelled,
            Ok(PlaybackEnd::EndOfFile { frames: 0 }) => return SourceEnd::Unavailable,
            Ok(PlaybackEnd::EndOfFile { .. }) => {}
            Err(error) => return SourceEnd::Failed(error),
        }
        match cancellation.state(generation) {
            CancellationState::Active => {}
            CancellationState::Replaced => return SourceEnd::Replaced,
            CancellationState::Closed => return SourceEnd::Cancelled,
        }
        decoder.flush();
        if source.input.seek(0, ..).is_err() {
            return SourceEnd::Unavailable;
        }
        frames.rewind(request.fps, source_fps);
    }
}

fn play_once(
    source: &mut VideoSource,
    decoder: &mut ff::decoder::Video,
    hardware_format: Option<ff::format::Pixel>,
    frames: &mut FramePipeline,
    token: u32,
    output: &mut impl std::io::Write,
    cancellation: &Cancellation,
    generation: u64,
) -> Result<PlaybackEnd, PlaybackFailure> {
    let stream_index = source.stream_index;
    let time_base_seconds = source.time_base_secs;
    let mut emitted = 0usize;
    for result in source.input.packets() {
        if let Some(end) = cancellation_end(cancellation, generation) {
            return Ok(end);
        }
        let (stream, packet) = result.map_err(PlaybackFailure::Demux)?;
        if stream.index() != stream_index {
            continue;
        }
        decoder.send_packet(&packet).map_err(PlaybackFailure::PacketSend)?;
        if let Some(end) = drain_playback(
            decoder,
            hardware_format,
            time_base_seconds,
            frames,
            token,
            output,
            cancellation,
            generation,
            &mut emitted,
        )? {
            return Ok(end);
        }
    }
    decoder.send_eof().map_err(PlaybackFailure::DrainStart)?;
    match drain_playback(
        decoder,
        hardware_format,
        time_base_seconds,
        frames,
        token,
        output,
        cancellation,
        generation,
        &mut emitted,
    )? {
        Some(end) => Ok(end),
        None => Err(PlaybackFailure::DrainIncomplete),
    }
}

fn drain_playback(
    decoder: &mut ff::decoder::Video,
    hardware_format: Option<ff::format::Pixel>,
    time_base_seconds: f64,
    frames: &mut FramePipeline,
    token: u32,
    output: &mut impl std::io::Write,
    cancellation: &Cancellation,
    generation: u64,
    emitted: &mut usize,
) -> Result<Option<PlaybackEnd>, PlaybackFailure> {
    loop {
        if let Some(end) = cancellation_end(cancellation, generation) {
            return Ok(Some(end));
        }
        match frames.next(decoder, hardware_format, time_base_seconds) {
            FrameOutcome::Frame { rgba, backend } => {
                if *emitted == 0 {
                    log::info!("stream-persist: observed frame backend {backend:?}");
                }
                output
                    .write_all(&token.to_le_bytes())
                    .and_then(|()| output.write_all(rgba))
                    .and_then(|()| output.flush())
                    .map_err(PlaybackFailure::Output)?;
                *emitted += 1;
            }
            FrameOutcome::Skipped => {}
            FrameOutcome::NeedPacket => return Ok(None),
            FrameOutcome::EndOfFile => {
                return Ok(Some(PlaybackEnd::EndOfFile { frames: *emitted }));
            }
            FrameOutcome::Failed(error) => return Err(map_frame_failure(error)),
        }
    }
}

fn cancellation_end(cancellation: &Cancellation, generation: u64) -> Option<PlaybackEnd> {
    match cancellation.state(generation) {
        CancellationState::Active => None,
        CancellationState::Replaced => Some(PlaybackEnd::Replaced),
        CancellationState::Closed => Some(PlaybackEnd::Cancelled),
    }
}

fn map_frame_failure(error: FrameFailure) -> PlaybackFailure {
    match error {
        FrameFailure::DecoderReceive(error) => PlaybackFailure::DecoderReceive(error),
        FrameFailure::HardwareTransfer(error) => PlaybackFailure::HardwareTransfer(error),
        FrameFailure::Scale(error) => PlaybackFailure::Scale(error),
    }
}

impl std::fmt::Display for PlaybackFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Demux(error) => write!(formatter, "demux error: {error}"),
            Self::PacketSend(error) => write!(formatter, "packet-send error: {error}"),
            Self::DrainStart(error) => write!(formatter, "decoder drain-start error: {error}"),
            Self::DrainIncomplete => {
                formatter.write_str("decoder requested a packet while draining")
            }
            Self::DecoderReceive(error) => write!(formatter, "decoder receive error: {error}"),
            Self::HardwareTransfer(error) => {
                write!(formatter, "hardware-frame transfer error: {error}")
            }
            Self::Scale(error) => write!(formatter, "frame scaling error: {error}"),
            Self::Output(error) => write!(formatter, "preview output error: {error}"),
        }
    }
}

#[cfg(test)]
mod tests;
