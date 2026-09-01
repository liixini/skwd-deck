use std::io::Read;
use std::time::Duration;

use super::{ext_from_url, safe_seg};

const PROGRESS_MIN_DELTA: f64 = 0.01;
const PROGRESS_MIN_INTERVAL: Duration = Duration::from_millis(100);

pub fn content_progress(done: u64, total: u64) -> Option<f64> {
    if total == 0 {
        return None;
    }
    Some((done as f64 / total as f64).clamp(0.0, 0.99))
}

pub fn download_with_progress(
    source: &str,
    full_url: &str,
    wallpaper_dir: &str,
    id: &str,
    on_progress: &mut dyn FnMut(f64),
) -> anyhow::Result<std::path::PathBuf> {
    let policy = |url: &str| crate::infrastructure::http::require_source(source, url);
    policy(full_url)?;
    let extension = safe_seg(ext_from_url(full_url));
    let id = safe_seg(id);
    let source = safe_seg(source);
    let destination =
        std::path::Path::new(wallpaper_dir).join(format!("{source}-{id}.{extension}"));
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let response = crate::infrastructure::http::get_guarded(full_url, policy)?;
    let total = response.header("Content-Length").and_then(|value| value.parse().ok()).unwrap_or(0);
    crate::infrastructure::http::require_length_within(
        &response,
        crate::infrastructure::http::MAX_DOWNLOAD_BYTES,
    )?;
    let mut reader = response.into_reader();
    let (mut file, temporary, cleanup) = crate::infrastructure::http::partial_file(&destination)?;
    match stream_to(
        &mut reader,
        &mut file,
        total,
        crate::infrastructure::http::MAX_DOWNLOAD_BYTES,
        on_progress,
    ) {
        Ok(()) => {
            drop(file);
            if let Err(error) = crate::infrastructure::sniff::check_file(
                &temporary,
                crate::infrastructure::sniff::Kind::Image,
            ) {
                let _ = std::fs::remove_file(&temporary);
                return Err(error);
            }
            std::fs::rename(&temporary, &destination)?;
            cleanup.commit();
            Ok(destination)
        }
        Err(error) => Err(error.into()),
    }
}

pub fn should_emit(percent: f64, last: f64, elapsed: Duration) -> bool {
    (percent - last) >= PROGRESS_MIN_DELTA && elapsed >= PROGRESS_MIN_INTERVAL
}

fn stream_to(
    reader: &mut dyn Read,
    file: &mut std::fs::File,
    total: u64,
    max: u64,
    on_progress: &mut dyn FnMut(f64),
) -> std::io::Result<()> {
    use std::io::Write;

    let mut buffer = vec![0_u8; 64 * 1024];
    let mut done = 0_u64;
    let mut last = 0.0;
    let mut last_at = std::time::Instant::now();
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            return Ok(());
        }
        let next = done
            .checked_add(read as u64)
            .ok_or_else(|| std::io::Error::other("remote body size overflow"))?;
        if next > max {
            return Err(std::io::Error::other(format!("remote body exceeds {max} byte limit")));
        }
        file.write_all(&buffer[..read])?;
        done = next;
        if let Some(percent) = content_progress(done, total)
            && should_emit(percent, last, last_at.elapsed())
        {
            last = percent;
            last_at = std::time::Instant::now();
            on_progress(percent);
        }
    }
}

#[cfg(test)]
#[path = "download/tests.rs"]
mod tests;
