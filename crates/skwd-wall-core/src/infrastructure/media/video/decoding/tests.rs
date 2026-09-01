use std::path::Path;

use super::{
    DecodeBackend, FrameOutcome, FramePipeline, MediaClock, ReceiveOutcome, classify_receive_error,
    open_fast_decoder,
};
use crate::infrastructure::media::video::source::VideoSource;

#[test]
fn missing_timestamps_advance_by_the_source_rate() {
    let mut clock = MediaClock::new(120.0);
    assert_eq!(clock.seconds(None, 0.0), 0.0);
    assert!((clock.seconds(None, 0.0) - 1.0 / 120.0).abs() < 1e-9);
}

#[test]
fn rewound_timestamps_keep_the_preview_clock_monotonic() {
    let mut clock = MediaClock::new(30.0);
    let step = 1.0 / 30.0;
    let seconds = [
        clock.seconds(Some(0), step),
        clock.seconds(None, step),
        clock.seconds(None, step),
        clock.seconds(Some(1), step),
        clock.seconds(Some(2), step),
        clock.seconds(Some(8), step),
    ];
    assert!(seconds.windows(2).all(|pair| pair[1] > pair[0]), "{seconds:?}");
    assert!((seconds[3] - 3.0 * step).abs() < 1e-9);
    assert!((seconds[5] - 8.0 * step).abs() < 1e-9);
}

#[test]
fn decoder_receive_outcomes_do_not_collapse() {
    assert_eq!(
        classify_receive_error(ffmpeg_the_third::Error::Other { errno: libc::EAGAIN }),
        ReceiveOutcome::NeedPacket
    );
    assert_eq!(classify_receive_error(ffmpeg_the_third::Error::Eof), ReceiveOutcome::EndOfFile);
    assert_eq!(
        classify_receive_error(ffmpeg_the_third::Error::InvalidData),
        ReceiveOutcome::Failed(ffmpeg_the_third::Error::InvalidData)
    );
}

#[test]
#[ignore = "manual live preview decode benchmark; set SKWD_BENCH_VIDEO"]
fn live_preview_decoder_benchmark() {
    let path = std::env::var("SKWD_BENCH_VIDEO").expect("set SKWD_BENCH_VIDEO");
    let source_path = Path::new(&path);
    let mut source = VideoSource::open(source_path).unwrap();
    let source_fps = source.frame_rate(30);
    let started = std::time::Instant::now();
    let cpu_started = process_cpu_seconds();
    let memory_before = skwd_log::proc::mem_breakdown();
    let (mut decoder, _hardware, hardware_format) =
        open_fast_decoder(&source, source_fps, 30).unwrap();
    let mut frames = FramePipeline::new(640, 360, 30, source_fps);
    let stream_index = source.stream_index;
    let time_base_seconds = source.time_base_secs;
    let mut bytes = 0usize;
    for result in source.input.packets() {
        let Ok((stream, packet)) = result else { break };
        if stream.index() != stream_index || decoder.send_packet(&packet).is_err() {
            continue;
        }
        loop {
            match frames.next(&mut decoder, hardware_format, time_base_seconds) {
                FrameOutcome::Frame { rgba, .. } => {
                    bytes = rgba.len();
                    break;
                }
                FrameOutcome::Skipped => {}
                FrameOutcome::NeedPacket | FrameOutcome::EndOfFile => break,
                FrameOutcome::Failed(error) => panic!("decode failed: {error:?}"),
            }
        }
        if bytes > 0 {
            break;
        }
    }
    let memory_after = skwd_log::proc::mem_breakdown();
    eprintln!(
        "live-preview-decoder backend={} elapsed_ms={} cpu_ms={:.1} rss_kb={} pss_kb={} rss_delta_kb={} pss_delta_kb={} frame_bytes={bytes}",
        match frames.observed_backend() {
            Some(DecodeBackend::HardwareTransfer) => "hardware-transfer",
            Some(DecodeBackend::Software) => "software-frame",
            None => "unobserved",
        },
        started.elapsed().as_millis(),
        (process_cpu_seconds() - cpu_started) * 1_000.0,
        memory_after.rss_kb,
        memory_after.pss_kb,
        memory_after.rss_kb.saturating_sub(memory_before.rss_kb),
        memory_after.pss_kb.saturating_sub(memory_before.pss_kb),
    );
    assert_eq!(bytes, 640 * 360 * 4);
}

fn process_cpu_seconds() -> f64 {
    let mut time = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut time) };
    time.tv_sec as f64 + time.tv_nsec as f64 / 1_000_000_000.0
}
