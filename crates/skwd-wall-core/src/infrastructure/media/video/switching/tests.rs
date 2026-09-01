use std::io::{Cursor, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::{
    PersistState, PlaybackEnd, PlaybackFailure, SourceEnd, StreamRequest,
    parse_request_with_default, play_once, read_requests, stream_source, stream_source_with,
};
use crate::infrastructure::media::video::cancellation::Cancellation;
use crate::infrastructure::media::video::decoding::{FramePipeline, open_persistent_decoder};
use crate::infrastructure::media::video::source::VideoSource;

#[test]
fn persistent_request_accepts_frame_cap() {
    assert_eq!(
        parse_request_with_default("/wall/video.mp4\t42\t20", 30),
        Some(StreamRequest { path: "/wall/video.mp4".to_string(), token: 42, fps: 20 })
    );
}

#[test]
fn request_cap_bounds() {
    assert_eq!(parse_request_with_default("a\t1", 20).unwrap().fps, 20);
    assert_eq!(parse_request_with_default("a\t1\t0", 30).unwrap().fps, 1);
    assert_eq!(parse_request_with_default("a\t1\t999", 30).unwrap().fps, 60);
    assert_eq!(parse_request_with_default("", 30), None);
}

#[test]
fn eof_leaves_the_last_request_owned_until_lifecycle_close() {
    let state = PersistState::new();
    read_requests(Cursor::new("/wall/a.mp4\t7\t20\n"), &state);
    assert_eq!(
        state.request.lock().unwrap().as_ref(),
        Some(&StreamRequest { path: "/wall/a.mp4".to_string(), token: 7, fps: 20 })
    );
    assert!(!state.cancellation.closed());
}

#[test]
fn empty_request_releases_the_active_source() {
    let state = PersistState::new();
    read_requests(Cursor::new("/wall/a.mp4\n\n"), &state);
    assert!(state.request.lock().unwrap().is_none());
}

struct ReplaceOnFlush<'a> {
    cancellation: &'a Cancellation,
    writes: usize,
}

impl Write for ReplaceOnFlush<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.writes += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.cancellation.replace();
        Ok(())
    }
}

#[test]
fn warm_replacement_releases_the_active_decode_and_frame_buffer() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.y4m");
    write_y4m(&source, 64, 36, 3);
    let cancellation = Cancellation::new();
    let generation = cancellation.snapshot();
    let request = StreamRequest { path: source.to_string_lossy().into_owned(), token: 17, fps: 60 };
    let mut output = ReplaceOnFlush { cancellation: &cancellation, writes: 0 };
    let releases = Arc::new(AtomicUsize::new(0));
    let probe = Arc::clone(&releases);
    let end = stream_source_with(
        &request,
        32,
        18,
        &mut output,
        &mut None,
        &cancellation,
        generation,
        move |width, height| FramePipeline::new(width, height, 60, 25.0).with_release_probe(probe),
    );
    assert!(matches!(end, SourceEnd::Replaced));
    assert_eq!(output.writes, 4 + 32 * 18 * 4);
    assert_eq!(releases.load(Ordering::SeqCst), 1);
}

#[test]
fn replaced_source_is_rejected_before_open_or_frame_allocation() {
    let cancellation = Cancellation::new();
    let generation = cancellation.snapshot();
    cancellation.replace();
    let allocations = Arc::new(AtomicUsize::new(0));
    let allocation_probe = Arc::clone(&allocations);
    let request =
        StreamRequest { path: "/path-that-must-not-be-opened".to_string(), token: 0, fps: 30 };
    let end = stream_source_with(
        &request,
        32,
        18,
        &mut Vec::new(),
        &mut None,
        &cancellation,
        generation,
        move |width, height| {
            allocation_probe.fetch_add(1, Ordering::SeqCst);
            FramePipeline::new(width, height, 30, 30.0)
        },
    );
    assert!(matches!(end, SourceEnd::Replaced));
    assert_eq!(allocations.load(Ordering::SeqCst), 0);
}

#[test]
fn malformed_source_has_an_explicit_unavailable_outcome() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("malformed.y4m");
    std::fs::write(&source, b"not video").unwrap();
    let cancellation = Cancellation::new();
    let request = StreamRequest { path: source.to_string_lossy().into_owned(), token: 0, fps: 30 };
    let end = stream_source(
        &request,
        32,
        18,
        &mut Vec::new(),
        &mut None,
        &cancellation,
        cancellation.snapshot(),
    );
    assert!(matches!(end, SourceEnd::Unavailable));
}

#[test]
fn finite_decode_pass_returns_owned_frames_at_end_of_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.y4m");
    write_y4m(&path, 64, 36, 3);
    let mut source = VideoSource::open(&path).unwrap();
    let source_fps = source.frame_rate(30);
    let mut hardware = None;
    let (mut decoder, hardware_format) =
        open_persistent_decoder(&source, source_fps, 60, &mut hardware).unwrap();
    let releases = Arc::new(AtomicUsize::new(0));
    let mut frames =
        FramePipeline::new(32, 18, 60, source_fps).with_release_probe(Arc::clone(&releases));
    let cancellation = Cancellation::new();
    let end = play_once(
        &mut source,
        &mut decoder,
        hardware_format,
        &mut frames,
        4,
        &mut Vec::new(),
        &cancellation,
        cancellation.snapshot(),
    )
    .unwrap();
    assert!(matches!(end, PlaybackEnd::EndOfFile { frames } if frames > 0));
    assert_eq!(releases.load(Ordering::SeqCst), 0);
    drop(frames);
    assert_eq!(releases.load(Ordering::SeqCst), 1);
}

struct CloseOnFlush<'a> {
    cancellation: &'a Cancellation,
}

impl Write for CloseOnFlush<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.cancellation.close();
        Ok(())
    }
}

#[test]
fn active_close_is_cancelled_and_releases_the_owned_frame_pipeline() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.y4m");
    write_y4m(&path, 64, 36, 3);
    let cancellation = Cancellation::new();
    let generation = cancellation.snapshot();
    let request = StreamRequest { path: path.to_string_lossy().into_owned(), token: 2, fps: 60 };
    let releases = Arc::new(AtomicUsize::new(0));
    let probe = Arc::clone(&releases);
    let end = stream_source_with(
        &request,
        32,
        18,
        &mut CloseOnFlush { cancellation: &cancellation },
        &mut None,
        &cancellation,
        generation,
        move |width, height| FramePipeline::new(width, height, 60, 25.0).with_release_probe(probe),
    );
    assert!(matches!(end, SourceEnd::Cancelled));
    assert_eq!(releases.load(Ordering::SeqCst), 1);
}

struct FailOutput;

impl Write for FailOutput {
    fn write(&mut self, _bytes: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "closed"))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn output_error_stays_typed_and_releases_the_owned_frame_pipeline() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.y4m");
    write_y4m(&path, 64, 36, 3);
    let cancellation = Cancellation::new();
    let generation = cancellation.snapshot();
    let request = StreamRequest { path: path.to_string_lossy().into_owned(), token: 2, fps: 60 };
    let releases = Arc::new(AtomicUsize::new(0));
    let probe = Arc::clone(&releases);
    let end = stream_source_with(
        &request,
        32,
        18,
        &mut FailOutput,
        &mut None,
        &cancellation,
        generation,
        move |width, height| FramePipeline::new(width, height, 60, 25.0).with_release_probe(probe),
    );
    assert!(matches!(end, SourceEnd::Failed(PlaybackFailure::Output(_))));
    assert_eq!(releases.load(Ordering::SeqCst), 1);
}

#[test]
fn packet_demux_receive_and_drain_failures_have_distinct_types() {
    assert!(
        format!("{}", PlaybackFailure::Demux(ffmpeg_the_third::Error::InvalidData))
            .starts_with("demux error:")
    );
    assert!(
        format!("{}", PlaybackFailure::PacketSend(ffmpeg_the_third::Error::InvalidData))
            .starts_with("packet-send error:")
    );
    assert!(
        format!("{}", PlaybackFailure::DecoderReceive(ffmpeg_the_third::Error::InvalidData))
            .starts_with("decoder receive error:")
    );
    assert_eq!(
        format!("{}", PlaybackFailure::DrainIncomplete),
        "decoder requested a packet while draining"
    );
}

#[test]
fn packet_send_failure_propagates_and_drops_its_frame_owner() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.y4m");
    write_y4m(&path, 64, 36, 3);
    let mut source = VideoSource::open(&path).unwrap();
    let source_fps = source.frame_rate(30);
    let mut hardware = None;
    let (mut decoder, hardware_format) =
        open_persistent_decoder(&source, source_fps, 60, &mut hardware).unwrap();
    decoder.send_eof().unwrap();
    let releases = Arc::new(AtomicUsize::new(0));
    let mut frames =
        FramePipeline::new(32, 18, 60, source_fps).with_release_probe(Arc::clone(&releases));
    let cancellation = Cancellation::new();
    let result = play_once(
        &mut source,
        &mut decoder,
        hardware_format,
        &mut frames,
        0,
        &mut Vec::new(),
        &cancellation,
        cancellation.snapshot(),
    );
    assert!(matches!(result, Err(PlaybackFailure::PacketSend(_))));
    drop(frames);
    assert_eq!(releases.load(Ordering::SeqCst), 1);
}

#[test]
#[ignore = "manual persistent lifecycle benchmark; set SKWD_BENCH_VIDEO"]
fn persistent_lifecycle_benchmark() {
    let path = std::env::var("SKWD_BENCH_VIDEO").expect("set SKWD_BENCH_VIDEO");
    let request = StreamRequest { path, token: 9, fps: 30 };
    let mut hardware = None;
    let prime = Cancellation::new();
    let prime_generation = prime.snapshot();
    let _ = stream_source(
        &request,
        640,
        360,
        &mut ReplaceOnFlush { cancellation: &prime, writes: 0 },
        &mut hardware,
        &prime,
        prime_generation,
    );

    let switched = Cancellation::new();
    let switched_generation = switched.snapshot();
    let backend = Arc::new(AtomicUsize::new(0));
    let backend_probe = Arc::clone(&backend);
    let before = BenchSnapshot::now();
    let end = stream_source_with(
        &request,
        640,
        360,
        &mut ReplaceOnFlush { cancellation: &switched, writes: 0 },
        &mut hardware,
        &switched,
        switched_generation,
        move |width, height| {
            FramePipeline::new(width, height, 30, 30.0).with_backend_probe(backend_probe)
        },
    );
    assert!(matches!(end, SourceEnd::Replaced));
    before.print("warm-switch", backend.load(Ordering::SeqCst));

    let cancelled = Cancellation::new();
    let cancelled_generation = cancelled.snapshot();
    let backend = Arc::new(AtomicUsize::new(0));
    let backend_probe = Arc::clone(&backend);
    let before = BenchSnapshot::now();
    let end = stream_source_with(
        &request,
        640,
        360,
        &mut CloseOnFlush { cancellation: &cancelled },
        &mut hardware,
        &cancelled,
        cancelled_generation,
        move |width, height| {
            FramePipeline::new(width, height, 30, 30.0).with_backend_probe(backend_probe)
        },
    );
    assert!(matches!(end, SourceEnd::Cancelled));
    before.print("active-cancel", backend.load(Ordering::SeqCst));

    let closed = Cancellation::new();
    closed.close();
    let before = BenchSnapshot::now();
    let end = stream_source(
        &request,
        640,
        360,
        &mut Vec::new(),
        &mut hardware,
        &closed,
        closed.snapshot(),
    );
    assert!(matches!(end, SourceEnd::Cancelled));
    before.print("closed-lifecycle", 0);

    let idle_ms = std::env::var("SKWD_BENCH_IDLE_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(10_000);
    let idle = Cancellation::new();
    let before = BenchSnapshot::now();
    assert_eq!(
        idle.wait_until(
            idle.snapshot(),
            std::time::Instant::now() + std::time::Duration::from_millis(idle_ms)
        ),
        crate::infrastructure::media::video::cancellation::WakeReason::Deadline
    );
    before.print("idle-deadline", 0);
}

#[test]
#[ignore = "manual idle owner benchmark; set SKWD_BENCH_IDLE_MS"]
fn idle_wait_benchmark() {
    let idle_ms = std::env::var("SKWD_BENCH_IDLE_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(10_000);
    let idle = Cancellation::new();
    let before = BenchSnapshot::now();
    assert_eq!(
        idle.wait_until(
            idle.snapshot(),
            std::time::Instant::now() + std::time::Duration::from_millis(idle_ms)
        ),
        crate::infrastructure::media::video::cancellation::WakeReason::Deadline
    );
    before.print("idle-only", 0);
}

struct BenchSnapshot {
    wall: std::time::Instant,
    cpu: f64,
    memory: skwd_log::proc::MemBreakdown,
}

impl BenchSnapshot {
    fn now() -> Self {
        Self {
            wall: std::time::Instant::now(),
            cpu: process_cpu_seconds(),
            memory: skwd_log::proc::mem_breakdown(),
        }
    }

    fn print(self, operation: &str, backend: usize) {
        let after = skwd_log::proc::mem_breakdown();
        let backend = match backend {
            2 => "hardware-transfer",
            1 => "software-frame",
            _ => "none",
        };
        eprintln!(
            "video-pipeline-benchmark operation={operation} backend={backend} elapsed_ms={} cpu_ms={:.1} rss_kb={} pss_kb={} rss_delta_kb={} pss_delta_kb={}",
            self.wall.elapsed().as_millis(),
            (process_cpu_seconds() - self.cpu) * 1_000.0,
            after.rss_kb,
            after.pss_kb,
            after.rss_kb.saturating_sub(self.memory.rss_kb),
            after.pss_kb.saturating_sub(self.memory.pss_kb),
        );
    }
}

fn process_cpu_seconds() -> f64 {
    let mut time = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut time) };
    time.tv_sec as f64 + time.tv_nsec as f64 / 1_000_000_000.0
}

fn write_y4m(path: &std::path::Path, width: usize, height: usize, frames: usize) {
    let mut bytes = format!("YUV4MPEG2 W{width} H{height} F25:1 Ip A1:1 C420jpeg\n").into_bytes();
    for _ in 0..frames {
        bytes.extend_from_slice(b"FRAME\n");
        bytes.extend(std::iter::repeat_n(180u8, width * height));
        bytes.extend(std::iter::repeat_n(128u8, width * height / 2));
    }
    std::fs::write(path, bytes).unwrap();
}
