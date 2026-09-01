use std::path::Path;
use std::process::{Command, Stdio};

pub fn ffmpeg_still(dest: &Path, spec: &str) -> bool {
    Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-nostdin",
            "-y",
            "-f",
            "lavfi",
            "-i",
            spec,
            "-frames:v",
            "1",
            "-update",
            "1",
        ])
        .arg(dest)
        .stdin(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub fn ffmpeg_video(dest: &Path, color: &str, secs: f64) -> bool {
    Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-nostdin",
            "-y",
            "-f",
            "lavfi",
            "-i",
            &format!("color=c={color}:s=320x180:d={secs}:r=24"),
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(dest)
        .stdin(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}
