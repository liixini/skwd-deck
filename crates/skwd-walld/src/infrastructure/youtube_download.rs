use std::io::{BufRead, BufReader, Read};
use std::process::Stdio;
use std::sync::Arc;

use wall_proto::ev;

use crate::backend::events::EventPublisher;
use crate::infrastructure::sources::youtube;

const MAX_STAT_LINE: usize = 8192;

fn dl_event(publisher: &dyn EventPublisher, id: &str, status: &str, progress: f64, message: &str) {
    let payload = wall_proto::DownloadEvent {
        progress: Some(progress),
        message: (!message.is_empty()).then(|| message.to_string()),
        ..wall_proto::DownloadEvent::new(id, status)
    };
    publisher.publish(ev::DOWNLOAD, payload.to_value());
}

fn dl_done(publisher: &dyn EventPublisher, id: &str, path: &str) {
    let payload = wall_proto::DownloadEvent {
        progress: Some(1.0),
        path: Some(path.to_string()),
        ..wall_proto::DownloadEvent::new(id, wall_proto::dl_status::DONE)
    };
    publisher.publish(ev::DOWNLOAD, payload.to_value());
}

pub(crate) fn finished_video(video_dir: &str, id: &str) -> Option<std::path::PathBuf> {
    let prefix = format!("youtube-{id}.");
    std::fs::read_dir(video_dir).ok()?.filter_map(Result::ok).map(|entry| entry.path()).find(
        |path| {
            path.file_name().is_some_and(|name| name.to_string_lossy().starts_with(&prefix))
                && path.extension().and_then(|ext| ext.to_str()) != Some("part")
        },
    )
}

pub(crate) fn extract_percent(line: &str) -> Option<f64> {
    let rest = line.strip_prefix("[download]")?.trim_start();
    let pct = rest.split('%').next()?.trim();
    if pct.is_empty() || !pct.starts_with(|ch: char| ch.is_ascii_digit()) {
        return None;
    }
    pct.parse::<f64>().ok().map(|val| (val / 100.0).clamp(0.0, 1.0))
}

pub(crate) fn hhmmss(secs: u64) -> String {
    format!("{}:{:02}:{:02}", secs / 3600, (secs % 3600) / 60, secs % 60)
}

#[derive(Default)]
pub(crate) struct Phases {
    pub seen: u32,
    pub merging: bool,
}

impl Phases {
    pub fn label(&self) -> &'static str {
        if self.merging {
            "merging"
        } else if self.seen <= 1 {
            "video"
        } else {
            "audio"
        }
    }

    pub fn overall(&self, pct: f64) -> f64 {
        if self.merging {
            return 0.98;
        }
        match self.seen {
            0 | 1 => pct * 0.90,
            _ => 0.90 + pct * 0.08,
        }
    }
}

pub(crate) fn note_line(ph: &mut Phases, line: &str) -> Option<f64> {
    if line.starts_with("[download] Destination:") {
        ph.seen += 1;
        return None;
    }
    if line.starts_with("[Merger]") || line.starts_with("[VideoRemuxer]") {
        ph.merging = true;
        return Some(0.98);
    }
    extract_percent(line).map(|pct| ph.overall(pct))
}

pub(crate) fn download_args(
    id: &str,
    video_dir: &str,
    max_height: u32,
    start_secs: u64,
    dur_secs: u64,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-f".into(),
        format!("bv*[height<=?{max_height}]+ba/b[height<=?{max_height}]/b"),
        "--merge-output-format".into(),
        "mp4".into(),
        "--no-playlist".into(),
        "--newline".into(),
        "--no-warnings".into(),
        "-o".into(),
        format!("{video_dir}/youtube-{id}.%(ext)s"),
    ];
    if dur_secs > 0 {
        let end = start_secs.saturating_add(dur_secs);
        args.push("--download-sections".into());
        args.push(format!("*{}-{}", hhmmss(start_secs), hhmmss(end)));
    }
    args.push(youtube::watch_url(id));
    args
}

pub(crate) fn parse_ffmpeg_time(line: &str) -> Option<f64> {
    let ts = line.split("time=").nth(1)?.split_whitespace().next()?;
    let mut it = ts.split(':');
    let hr: f64 = it.next()?.parse().ok()?;
    let min: f64 = it.next()?.parse().ok()?;
    let sec: f64 = it.next()?.parse().ok()?;
    Some(hr.mul_add(3600.0, min.mul_add(60.0, sec)))
}

pub(crate) fn clip_progress(line: &str, dur_secs: u64) -> Option<f64> {
    if dur_secs == 0 {
        return None;
    }
    let secs = parse_ffmpeg_time(line)?;
    Some((secs / dur_secs as f64).clamp(0.0, 0.98))
}

fn for_each_stat_line<R: std::io::Read>(src: R, mut on_line: impl FnMut(&str)) {
    let mut br = BufReader::new(src);
    let mut buf: Vec<u8> = Vec::with_capacity(256);
    let mut byte = [0u8; 1];
    while matches!(br.read(&mut byte), Ok(1)) {
        if byte[0] == b'\r' || byte[0] == b'\n' {
            if !buf.is_empty() {
                on_line(&String::from_utf8_lossy(&buf));
                buf.clear();
            }
        } else if buf.len() < MAX_STAT_LINE {
            buf.push(byte[0]);
        }
    }
    if !buf.is_empty() {
        on_line(&String::from_utf8_lossy(&buf));
    }
}

pub(crate) fn run_download(
    id: &str,
    video_dir: &str,
    max_height: u32,
    start_secs: u64,
    dur_secs: u64,
    publisher: &Arc<dyn EventPublisher>,
) -> bool {
    if !youtube::safe_id(id) {
        dl_event(publisher.as_ref(), id, wall_proto::dl_status::ERROR, 0.0, "invalid youtube id");
        return false;
    }
    let _ = std::fs::create_dir_all(video_dir);
    if dur_secs > 0 {
        let end = start_secs.saturating_add(dur_secs);
        log::info!("youtube: clipping {id} to {}-{}", hhmmss(start_secs), hhmmss(end));
    }
    let mut cmd = crate::infrastructure::proc::tool(youtube::BIN);
    cmd.args(download_args(id, video_dir, max_height, start_secs, dur_secs))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let Some(mut child) = spawn_yt(&mut cmd, publisher.as_ref(), id) else {
        return false;
    };
    dl_event(publisher.as_ref(), id, wall_proto::dl_status::DOWNLOADING, 0.0, "video");
    let drain = child
        .stderr
        .take()
        .map(|stderr| spawn_clip_drain(stderr, dur_secs, Arc::clone(publisher), id));
    if let Some(stdout) = child.stdout.take() {
        pump_progress(stdout, dur_secs, publisher.as_ref(), id);
    }
    if let Some(handle) = drain {
        let _ = handle.join();
    }
    let ok = matches!(child.wait(), Ok(status) if status.success());
    if !ok {
        dl_event(
            publisher.as_ref(),
            id,
            wall_proto::dl_status::ERROR,
            0.0,
            "yt-dlp download failed",
        );
        return false;
    }
    if let Some(file) = finished_video(video_dir, id) {
        if let Err(err) = crate::infrastructure::sniff::check_file(
            &file,
            crate::infrastructure::sniff::Kind::Video,
        ) {
            let _ = std::fs::remove_file(&file);
            dl_event(
                publisher.as_ref(),
                id,
                wall_proto::dl_status::ERROR,
                0.0,
                &format!("downloaded file rejected: {err}"),
            );
            return false;
        }
        let path = file.to_string_lossy().into_owned();
        log::info!("youtube: downloaded {id} -> {path}");
        dl_done(publisher.as_ref(), id, &path);
        true
    } else {
        dl_event(
            publisher.as_ref(),
            id,
            wall_proto::dl_status::ERROR,
            0.0,
            "download finished but no file appeared",
        );
        false
    }
}

fn spawn_yt(
    cmd: &mut std::process::Command,
    publisher: &dyn EventPublisher,
    id: &str,
) -> Option<std::process::Child> {
    match cmd.spawn() {
        Ok(child) => Some(child),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            dl_event(
                publisher,
                id,
                wall_proto::dl_status::ERROR,
                0.0,
                "yt-dlp not found - install it to download",
            );
            None
        }
        Err(err) => {
            dl_event(
                publisher,
                id,
                wall_proto::dl_status::ERROR,
                0.0,
                &format!("yt-dlp spawn failed: {err}"),
            );
            None
        }
    }
}

fn spawn_clip_drain(
    stderr: std::process::ChildStderr,
    dur_secs: u64,
    publisher: Arc<dyn EventPublisher>,
    id: &str,
) -> std::thread::JoinHandle<()> {
    let id = id.to_string();
    std::thread::spawn(move || {
        let mut last = 0.0f64;
        for_each_stat_line(stderr, |line| {
            if let Some(pct) = clip_progress(line, dur_secs)
                && (pct - last) >= 0.005
            {
                last = pct;
                dl_event(
                    publisher.as_ref(),
                    &id,
                    wall_proto::dl_status::DOWNLOADING,
                    pct,
                    "clipping",
                );
            }
        });
    })
}

fn pump_progress(
    stdout: std::process::ChildStdout,
    dur_secs: u64,
    publisher: &dyn EventPublisher,
    id: &str,
) {
    let mut ph = Phases::default();
    let mut last = 0.0f64;
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if dur_secs > 0 {
            continue;
        }
        if let Some(pct) = note_line(&mut ph, &line)
            && (pct - last) >= 0.005
        {
            last = pct;
            dl_event(publisher, id, wall_proto::dl_status::DOWNLOADING, pct, ph.label());
        }
    }
}

mod tests;
