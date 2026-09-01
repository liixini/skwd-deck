#![cfg(test)]

use super::*;

#[test]
fn percent_from_progress() {
    assert_eq!(extract_percent("[download]   0.0% of  12.34MiB at 1.2MiB/s ETA 00:10"), Some(0.0));
    assert_eq!(
        extract_percent("[download]  45.2% of  12.34MiB at 1.2MiB/s ETA 00:05"),
        Some(0.452)
    );
    assert_eq!(extract_percent("[download] 100% of 12.34MiB in 00:10"), Some(1.0));
}

#[test]
fn clip_progress_time() {
    let real = "frame=  192 fps=0.0 q=31.0 size=     512KiB time=00:00:03.16 bitrate=1324.6kbits/s speed=6.33x elapsed=0:00:00.50";
    assert_eq!(parse_ffmpeg_time(real), Some(3.16));
    let pct = clip_progress(real, 10).expect("clip progress");
    assert!((pct - 0.316).abs() < 1e-9);

    assert_eq!(parse_ffmpeg_time("time=01:02:03.00"), Some(3723.0));
    assert_eq!(parse_ffmpeg_time("time=N/A"), None);
    assert_eq!(parse_ffmpeg_time("frame= 1 fps=0.0"), None);
    assert_eq!(clip_progress(real, 0), None);
    assert_eq!(clip_progress("time=00:00:12.00", 10), Some(0.98));
}

#[test]
fn stat_lines_cr() {
    let mut seen: Vec<String> = Vec::new();
    let raw = "Input #0, mov\rtime=00:00:01.00\rtime=00:00:02.00\nlast";
    super::for_each_stat_line(std::io::Cursor::new(raw), |line| seen.push(line.to_string()));
    assert_eq!(seen, ["Input #0, mov", "time=00:00:01.00", "time=00:00:02.00", "last"]);
}

#[test]
fn clip_args_no_reencode() {
    let args = download_args("abc123", "/vid", 1440, 300, 180);
    let idx = args.iter().position(|arg| arg == "--download-sections").expect("clip requested");
    assert_eq!(args[idx + 1], "*0:05:00-0:08:00");
    assert!(!args.iter().any(|arg| arg == "--force-keyframes-at-cuts"));
    assert!(args.last().is_some_and(|arg| arg.contains("abc123")));

    let full = download_args("abc123", "/vid", 1440, 0, 0);
    assert!(!full.iter().any(|arg| arg == "--download-sections"));
}

#[test]
fn hhmmss_format() {
    assert_eq!(hhmmss(0), "0:00:00");
    assert_eq!(hhmmss(180), "0:03:00");
    assert_eq!(hhmmss(37251), "10:20:51");
}

#[test]
fn phases_monotonic() {
    let mut ph = Phases::default();
    let mut seen: Vec<f64> = Vec::new();
    for line in [
        "[download] Destination: /w/youtube-x.f399.mp4",
        "[download]   0.0% of 12.00MiB at 1MiB/s ETA 00:12",
        "[download]  50.0% of 12.00MiB at 1MiB/s ETA 00:06",
        "[download] 100% of 12.00MiB in 00:12",
        "[download] Destination: /w/youtube-x.f251.webm",
        "[download]   0.0% of 1.00MiB at 1MiB/s ETA 00:01",
        "[download] 100% of 1.00MiB in 00:01",
        "[Merger] Merging formats into \"/w/youtube-x.mp4\"",
    ] {
        if let Some(pct) = note_line(&mut ph, line) {
            seen.push(pct);
        }
    }
    assert!(seen.windows(2).all(|pair| pair[1] >= pair[0]), "{seen:?}");
    assert_eq!(seen.first().copied(), Some(0.0));
    assert!((seen[2] - 0.90).abs() < 1e-9);
    assert!((seen.last().copied().unwrap() - 0.98).abs() < 1e-9);
}

#[test]
fn phase_labels() {
    let mut ph = Phases::default();
    assert_eq!(ph.label(), "video");
    note_line(&mut ph, "[download] Destination: /w/a.f399.mp4");
    assert_eq!(ph.label(), "video");
    note_line(&mut ph, "[download] Destination: /w/a.f251.webm");
    assert_eq!(ph.label(), "audio");
    note_line(&mut ph, "[Merger] Merging formats into \"/w/a.mp4\"");
    assert_eq!(ph.label(), "merging");
}

#[test]
fn finished_skips_part() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_str().unwrap();

    std::fs::write(tmp.path().join("youtube-abc123.mp4.part"), b"half").unwrap();
    assert!(finished_video(dir, "abc123").is_none());

    std::fs::write(tmp.path().join("youtube-abc123.mp4"), b"whole").unwrap();
    assert_eq!(finished_video(dir, "abc123").unwrap().extension().unwrap(), "mp4");
    assert!(finished_video(dir, "nope").is_none());
}

#[test]
fn percent_ignores_noise() {
    assert_eq!(extract_percent("[youtube] dQw4w9WgXcQ: Downloading webpage"), None);
    assert_eq!(extract_percent("[download] Destination: /w/youtube-abc.mp4"), None);
    assert_eq!(extract_percent("[Merger] Merging formats into \"/w/youtube-abc.mp4\""), None);
    assert_eq!(extract_percent(""), None);
    assert_eq!(extract_percent("45.2%"), None);
}
