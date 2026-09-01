use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};

pub(super) const OPT_THREADS: &str = "2";

pub(crate) fn decode_probe_args(src: &str, codec: &str) -> Vec<String> {
    let mut args = vec![String::from("-v"), String::from("error"), String::from("-nostdin")];
    if codec == "av1" {
        args.extend([String::from("-c:v"), String::from("libdav1d")]);
    }
    args.extend([
        String::from("-i"),
        src.into(),
        String::from("-map"),
        String::from("0:v:0"),
        String::from("-frames:v"),
        String::from("1"),
        String::from("-f"),
        String::from("null"),
        String::from("-"),
    ]);
    args
}

pub(crate) fn decode_probe_ok(src: &str, codec: &str) -> bool {
    crate::infrastructure::proc::tool("ffmpeg")
        .args(decode_probe_args(src, codec))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub(crate) fn tinier_dest_path(cache_dir: &Path, src: &str) -> PathBuf {
    let stem = Path::new(src)
        .file_stem()
        .map_or_else(|| String::from("video"), |stem| stem.to_string_lossy().to_string());
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in src.as_bytes() {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
    }
    cache_dir.join("video-opt").join(format!("{stem}-{hash:08x}.tinier-v1.ivf"))
}

pub(crate) fn tinier_encode_args(
    src: &str,
    dest: &str,
    max_height: u32,
    fps_cap: Option<u32>,
) -> Vec<String> {
    let mut filters = Vec::new();
    if let Some(fps) = fps_cap {
        filters.push(format!("fps={fps}"));
    }
    if max_height > 0 {
        filters.push(format!("scale=-2:'min(ih,{max_height})'"));
    }
    let mut args = vec![
        "-y".into(),
        "-v".into(),
        "error".into(),
        "-nostdin".into(),
        "-nostats".into(),
        "-progress".into(),
        "pipe:1".into(),
        "-hwaccel".into(),
        "auto".into(),
        "-threads".into(),
        OPT_THREADS.into(),
        "-i".into(),
        src.into(),
        "-map".into(),
        "0:v:0".into(),
    ];
    if !filters.is_empty() {
        args.extend(["-vf".into(), filters.join(",")]);
    }
    args.extend(
        [
            "-an",
            "-sn",
            "-dn",
            "-c:v",
            "libsvtav1",
            "-preset",
            "12",
            "-crf",
            "35",
            "-g",
            "60",
            "-pix_fmt",
            "yuv420p",
            "-svtav1-params",
            "lp=2:pred-struct=1",
            "-f",
            "ivf",
        ]
        .into_iter()
        .map(String::from),
    );
    args.push(dest.into());
    args
}

pub(crate) fn probe_duration_ms(src: &str) -> Option<u64> {
    let output = crate::infrastructure::proc::tool("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            src,
        ])
        .output()
        .ok()?;
    let seconds = String::from_utf8(output.stdout).ok()?.trim().parse::<f64>().ok()?;
    (seconds.is_finite() && seconds > 0.0).then_some((seconds * 1000.0).round() as u64)
}

pub(crate) fn probe_frame_rate(src: &str) -> Option<String> {
    let output = crate::infrastructure::proc::tool("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=avg_frame_rate,r_frame_rate",
            "-of",
            "default=noprint_wrappers=1",
            src,
        ])
        .output()
        .ok()?;
    frame_rate_from_probe(&String::from_utf8(output.stdout).ok()?)
}

pub(super) fn frame_rate_from_probe(output: &str) -> Option<String> {
    let value =
        |key| output.lines().find_map(|line| line.strip_prefix(key)).and_then(parse_frame_rate);
    value("avg_frame_rate=").or_else(|| value("r_frame_rate="))
}

fn parse_frame_rate(rate: &str) -> Option<String> {
    let (numerator, denominator) = rate.split_once('/')?;
    let numerator = numerator.parse::<u32>().ok()?;
    let denominator = denominator.parse::<u32>().ok()?;
    if numerator == 0 || denominator == 0 || f64::from(numerator) / f64::from(denominator) > 240.0 {
        return None;
    }
    Some(format!("{numerator}/{denominator}"))
}

pub(crate) fn probe(src: &str) -> Option<(String, u32, u32, f64)> {
    let output = crate::infrastructure::proc::tool("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_name,width,height,avg_frame_rate",
            "-of",
            "csv=p=0",
            src,
        ])
        .output()
        .ok()?;
    let line = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = line.trim().split(',').collect();
    if parts.len() < 4 {
        return None;
    }
    let codec = parts[0].to_string();
    let width = parts[1].parse().ok()?;
    let height = parts[2].parse().ok()?;
    let fps = match parts[3].split('/').collect::<Vec<_>>()[..] {
        [numerator, denominator] => {
            numerator.parse::<f64>().ok()? / denominator.parse::<f64>().ok()?.max(1.0)
        }
        _ => 24.0,
    };
    Some((codec, width, height, fps))
}

pub(crate) fn spawn_encoder(args: &[String]) -> std::io::Result<Child> {
    crate::infrastructure::proc::tool("nice")
        .arg("-n")
        .arg("15")
        .arg("ffmpeg")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
}
