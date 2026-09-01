use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use rayon::prelude::*;
use serde_json::json;
use skwd_wall_core::{media, paths};
use tokio::io::{AsyncBufReadExt, AsyncReadExt};
use wall_proto::rpc;

use crate::reporter::Reporter;

const MAX_REDIRECTS: u32 = 5;
const MAX_BYTES: u64 = 20_000_000;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_secs(20);
const FETCH_JOBS: usize = 6;
const DECODE_JOBS: usize = 3;
const MAX_JOBS: usize = 64;
const MAX_SPOOL_BYTES: u64 = 256 * 1024 * 1024;
const MAX_JOB_INPUT_BYTES: u64 = 1024 * 1024;

struct Downloaded {
    id: String,
    spool: std::path::PathBuf,
    output: std::path::PathBuf,
}

impl Drop for Downloaded {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.spool);
    }
}

pub(crate) async fn run(source: &str, reporter: &Reporter) -> anyhow::Result<()> {
    let mut jobs: Vec<(String, String)> = Vec::new();
    let mut lines = tokio::io::BufReader::new(tokio::io::stdin().take(MAX_JOB_INPUT_BYTES)).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if let Some(job) = parse_job(&line) {
            if jobs.len() < MAX_JOBS {
                jobs.push(job);
            } else {
                reporter.send(rpc::REMOTE_THUMB, &json!({ "id": job.0, "path": "" }));
            }
        }
    }
    log::info!("remote-thumb [{source}]: {} jobs", jobs.len());

    let downloaded = std::thread::scope(|scope| {
        scope
            .spawn(|| fetch_jobs(source, reporter, &jobs))
            .join()
            .map_err(|_| anyhow::anyhow!("remote thumbnail download worker panicked"))
    })??;
    if downloaded.is_empty() {
        return Ok(());
    }
    if let Err(error) = crate::sandbox::restrict_decode(
        &crate::sandbox::Policy::new().write(paths::cache_dir()).cpu_seconds(10 * 60),
    ) {
        cleanup_downloads(&downloaded);
        return Err(error);
    }
    let result = std::thread::scope(|scope| {
        scope
            .spawn(|| decode_jobs(reporter, &downloaded))
            .join()
            .map_err(|_| anyhow::anyhow!("remote thumbnail decode worker panicked"))
    });
    cleanup_downloads(&downloaded);
    result??;
    Ok(())
}

fn fetch_jobs(
    source: &str,
    reporter: &Reporter,
    jobs: &[(String, String)],
) -> anyhow::Result<Vec<Downloaded>> {
    let spool_bytes = AtomicU64::new(0);
    let work = || {
        jobs.par_iter()
            .enumerate()
            .filter_map(|(index, (id, url))| {
                let output = paths::remote_thumb(source, id);
                if output.exists() {
                    reporter.send(
                        rpc::REMOTE_THUMB,
                        &json!({ "id": id, "path": output.to_string_lossy() }),
                    );
                    return None;
                }
                if let Some(parent) = output.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                let spool = spool_path(&output, index);
                match fetch_bytes(url).and_then(|bytes| {
                    let length = bytes.len() as u64;
                    spool_bytes
                        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                            current.checked_add(length).filter(|total| *total <= MAX_SPOOL_BYTES)
                        })
                        .map_err(|_| anyhow::anyhow!("remote thumbnail spool budget exhausted"))?;
                    skwd_wall_core::paths::atomic_write_mode(&spool, &bytes, Some(0o600))?;
                    Ok(())
                }) {
                    Ok(()) => Some(Downloaded { id: id.clone(), spool, output }),
                    Err(error) => {
                        log::warn!("remote thumb {id} failed: {error}");
                        reporter.send(rpc::REMOTE_THUMB, &json!({ "id": id, "path": "" }));
                        None
                    }
                }
            })
            .collect()
    };
    let pool = rayon::ThreadPoolBuilder::new().num_threads(FETCH_JOBS).build()?;
    Ok(pool.install(work))
}

fn decode_jobs(reporter: &Reporter, jobs: &[Downloaded]) -> anyhow::Result<()> {
    let work = || {
        jobs.par_iter().for_each(|job| {
            let result = std::fs::read(&job.spool)
                .map_err(anyhow::Error::from)
                .and_then(|bytes| media::webp_from_bytes(&bytes, &job.output, 82.0, 440));
            let _ = std::fs::remove_file(&job.spool);
            match result {
                Ok(()) => reporter.send(
                    rpc::REMOTE_THUMB,
                    &json!({ "id": job.id, "path": job.output.to_string_lossy() }),
                ),
                Err(error) => {
                    log::warn!("remote thumb {} decode failed: {error}", job.id);
                    reporter.send(rpc::REMOTE_THUMB, &json!({ "id": job.id, "path": "" }));
                }
            }
        });
    };
    let pool = rayon::ThreadPoolBuilder::new().num_threads(DECODE_JOBS).build()?;
    pool.install(work);
    Ok(())
}

fn spool_path(output: &std::path::Path, index: usize) -> std::path::PathBuf {
    let mut name = output.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".download.{}.{index}", std::process::id()));
    output.with_file_name(name)
}

fn cleanup_downloads(jobs: &[Downloaded]) {
    for job in jobs {
        let _ = std::fs::remove_file(&job.spool);
    }
}

pub(crate) fn parse_job(line: &str) -> Option<(String, String)> {
    let (id, url) = line.split_once('\t')?;
    (!id.is_empty() && !url.is_empty()).then(|| (id.to_string(), url.to_string()))
}

pub(crate) use wall_proto::net::resolve_redirect;
#[cfg(test)]
pub(crate) use wall_proto::net::{host_is_private as host_private, scheme_host};

pub(crate) fn require_public(raw: &str) -> anyhow::Result<()> {
    wall_proto::net::require_public(raw).map_err(anyhow::Error::msg)
}

pub(crate) fn fetch_bytes(url: &str) -> anyhow::Result<Vec<u8>> {
    fetch_bytes_guarded(url, require_public)
}

#[cfg(debug_assertions)]
fn assert_blocking_http_context() {
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let drives_async =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handle.block_on(async {})))
            .is_err();
    assert!(!drives_async, "blocking ureq on tokio worker");
}

pub(crate) fn fetch_bytes_guarded<F>(url: &str, policy: F) -> anyhow::Result<Vec<u8>>
where
    F: Fn(&str) -> anyhow::Result<()>,
{
    #[cfg(debug_assertions)]
    assert_blocking_http_context();
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(CONNECT_TIMEOUT)
        .timeout_read(READ_TIMEOUT)
        .redirects(0)
        .build();
    let mut current = url.to_string();
    for _ in 0..=MAX_REDIRECTS {
        policy(&current)?;
        let response = agent.get(&current).call()?;
        if (300..400).contains(&response.status()) {
            let location = response
                .header("Location")
                .ok_or_else(|| anyhow::anyhow!("redirect without Location"))?;
            current = resolve_redirect(&current, location)
                .ok_or_else(|| anyhow::anyhow!("blocked bad redirect location: {location}"))?;
            continue;
        }
        let mut bytes = Vec::new();
        response.into_reader().take(MAX_BYTES).read_to_end(&mut bytes)?;
        return Ok(bytes);
    }
    anyhow::bail!("too many redirects")
}
