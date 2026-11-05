use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use skwd_config::GPU_DEVICE_ENV;
use skwd_wall_core::config::Config;
use skwd_wall_core::infrastructure::drawing as cache;
use wall_proto::drawing::{Device, Phase, Progress, arg, stage};

use crate::composition::context::Ctx;

use super::CpuFrom;
use super::pack::Pack;

const POLL_INTERVAL: Duration = Duration::from_millis(100);
const TIME_LIMIT: Duration = Duration::from_secs(300);
const MEMORY_LIMIT_KIB: u64 = 6 * 1024 * 1024;
const KIB_PER_MIB: u64 = 1024;
const ERROR_LOG: &str = "prepare.log";
const OUTPUT_DIRECTORY: &str = "result";
const REFERENCE_EXTENSION: &str = "webp";
const CAPTURE_ARG: &str = "capture-scenes";
const MEDIA_CHANGED: &str = "media changed during preparation";
const CAPTURE_FAILED: &str = "drawing reference capture failed";
const CAPTURE_SOURCE_MISMATCH: &str = "drawing reference source mismatch";
const CAPTURE_EMPTY: &str = "drawing reference is empty";
const CAPTURE_INVALID: &str = "drawing reference is not a valid image";
const STDERR_LIMIT: usize = 4096;
const RSS_PREFIX: &str = "VmRSS:";
const MAX_PROGRESS_LINE: usize = 4096;
const JOB_PREFIX: &str = "job-";

#[derive(Debug)]
struct MemoryExceeded;

impl std::fmt::Display for MemoryExceeded {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("drawing preparation exceeded its memory limit")
    }
}

impl std::error::Error for MemoryExceeded {}

struct ReferenceCleanup(Option<std::path::PathBuf>);

impl Drop for ReferenceCleanup {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}

struct ArtifactCleanup(Option<std::path::PathBuf>);

impl Drop for ArtifactCleanup {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

pub(super) fn validate(ctx: &Ctx, path: &Path, cancel: &AtomicBool) {
    let started = Instant::now();
    let result = (|| -> Result<()> {
        let config = ctx.state.config().clone();
        let pack = Pack::inspect_cancellable(path, cancel)?;
        let directory = cache::root(&config).join(cache::WORK_DIRECTORY);
        std::fs::create_dir_all(&directory)?;
        let temporary = tempfile::Builder::new().prefix(JOB_PREFIX).tempdir_in(directory)?;
        warn_invalid_settings(ctx, &config);
        let mut command = command(ctx, &pack, temporary.path(), device(&config, &pack))?;
        ensure!(!cancel.load(Ordering::Acquire), "processing check cancelled");
        let child = command.arg(arg::CHECK).spawn().context("check drawing processing files")?;
        supervise(ctx, child, memory_limit_kib(&config), cancel, started)
            .with_context(|| error_tail(&temporary.path().join(ERROR_LOG)))?;
        ensure!(!cancel.load(Ordering::Acquire), "processing check cancelled");
        pack.save(&config)
    })();
    let mut job = skwd_wall_core::lock(&ctx.drawing.job);
    if cancel.load(Ordering::Acquire) {
        return;
    }
    job.status.elapsed_ms = started.elapsed().as_millis() as u64;
    match result {
        Ok(()) => {
            ctx.state.reload_config();
            job.status.phase = Phase::Idle;
            job.status.progress = 1.0;
            job.status.pack = wall_proto::drawing::PackState::Available;
        }
        Err(error) => {
            job.status.phase = Phase::Failed;
            job.status.error = Some(format!("{error:#}"));
        }
    }
    job.cancel = None;
    super::publish(ctx, &job.status);
}

fn device(config: &Config, pack: &Pack) -> Device {
    match skwd_config::drawing_device(config.root()) {
        Device::Auto => pack.device,
        chosen => chosen,
    }
}

fn memory_limit_kib(config: &Config) -> u64 {
    skwd_config::drawing_memory_limit_mib(config.root())
        .map_or(MEMORY_LIMIT_KIB, |mib| mib * KIB_PER_MIB)
}

pub(super) fn warn_invalid_settings(ctx: &Ctx, config: &Config) {
    let invalid = [
        (skwd_config::keys::drawing::DEVICE, skwd_config::drawing_device_invalid(config.root())),
        (
            skwd_config::keys::drawing::MEMORY_LIMIT_MIB,
            skwd_config::drawing_memory_limit_invalid(config.root()),
        ),
    ];
    let mut job = skwd_wall_core::lock(&ctx.drawing.job);
    for (setting, value) in invalid {
        if let Some(value) = value
            && job.warned.insert(format!("{setting}={value}"))
        {
            log::warn!("drawing: ignoring unrecognised {setting} {value}, using auto");
        }
    }
}

fn command(ctx: &Ctx, pack: &Pack, directory: &Path, device: Device) -> Result<Command> {
    let mut command = skwd_wall_core::proc::tool(skwd_wall_core::paths::sibling_bin(
        skwd_wall_core::paths::DRAW_BIN,
    ));
    command
        .arg(arg::PACK)
        .arg(&pack.path)
        .arg(arg::DEVICE)
        .arg(device.label())
        .env(GPU_DEVICE_ENV, ctx.state.config().renderer().gpu_device())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(std::fs::File::create(directory.join(ERROR_LOG))?);
    Ok(command)
}

fn error_tail(path: &Path) -> String {
    let read = || -> std::io::Result<String> {
        let mut file = std::fs::File::open(path)?;
        let length = file.metadata()?.len();
        file.seek(SeekFrom::Start(length.saturating_sub(STDERR_LIMIT as u64)))?;
        let mut bytes = Vec::new();
        file.take(STDERR_LIMIT as u64).read_to_end(&mut bytes)?;
        Ok(String::from_utf8_lossy(&bytes).trim().to_string())
    };
    read().unwrap_or_default()
}

pub(super) fn prepare(ctx: &Ctx, pack: &Pack, cancel: &AtomicBool) {
    let started = Instant::now();
    let Err(error) = prepare_queue(ctx, pack, cancel, started) else {
        return;
    };
    let mut job = skwd_wall_core::lock(&ctx.drawing.job);
    if cancel.load(Ordering::Acquire) {
        return;
    }
    job.activity = super::Activity::Stopped;
    job.cancel = None;
    job.status.elapsed_ms = started.elapsed().as_millis() as u64;
    job.status.phase =
        if super::can_prepare(&ctx.state.config()) { Phase::Failed } else { Phase::Paused };
    job.status.error = Some(format!("{error:#}"));
    job.count();
    job.publish(ctx);
}

enum Next {
    Plan,
    Item(String),
    Done,
}

fn next(ctx: &Ctx, started: Instant) -> Next {
    let mut job = skwd_wall_core::lock(&ctx.drawing.job);
    if std::mem::take(&mut job.plan) {
        return Next::Plan;
    }
    if let Some(key) = job.pending.pop_front() {
        job.activity = super::Activity::Busy;
        job.status.enabled = skwd_config::drawing_enabled(ctx.state.config().root(), &key);
        job.status.active_key.clone_from(&key);
        job.status.key.clone_from(&key);
        job.status.prepared = false;
        job.status.progress = 0.0;
        return Next::Item(key);
    }
    job.activity = super::Activity::Stopped;
    job.cancel = None;
    job.status.elapsed_ms = started.elapsed().as_millis() as u64;
    job.status.phase = Phase::Ready;
    job.status.progress = 1.0;
    job.status.prepared = true;
    job.status.stage = stage::COMPLETE.to_string();
    job.publish(ctx);
    job.library = false;
    Next::Done
}

fn prepare_queue(ctx: &Ctx, pack: &Pack, cancel: &AtomicBool, started: Instant) -> Result<()> {
    let verified = Pack::inspect_cancellable(&pack.path, cancel)?;
    ensure!(
        verified.fingerprint == pack.fingerprint,
        "processing files changed; register them again"
    );
    loop {
        ensure!(!cancel.load(Ordering::Acquire), "preparation cancelled");
        let key = match next(ctx, started) {
            Next::Done => return Ok(()),
            Next::Plan => {
                super::batch::merge(ctx, super::batch::plan(ctx, &pack.fingerprint)?);
                continue;
            }
            Next::Item(key) => key,
        };
        let result = prepare_item(ctx, &key, pack, cancel);
        let mut job = skwd_wall_core::lock(&ctx.drawing.job);
        job.activity = super::Activity::Waiting;
        let enable = job.enable.remove(&key);
        let outline = job.outlines.remove(&key);
        match result {
            Ok(()) => job.status.completed += 1,
            Err(error)
                if job.library
                    && !cancel.load(Ordering::Acquire)
                    && super::can_prepare(&ctx.state.config()) =>
            {
                log::warn!("drawing: skipped {key}: {error:#}");
                job.failed.insert(key.clone());
                job.status.failed = job.failed.len() as u32;
                job.count();
                job.publish(ctx);
                continue;
            }
            Err(error)
                if outline
                    && !cancel.load(Ordering::Acquire)
                    && super::can_prepare(&ctx.state.config()) =>
            {
                log::warn!("drawing: no outline for {key}: {error:#}");
                job.count();
                job.publish(ctx);
                continue;
            }
            Err(error) => {
                if enable {
                    job.enable.insert(key.clone());
                }
                job.pending.push_front(key);
                return Err(error);
            }
        }
        if enable {
            drop(job);
            super::preference::save(ctx, &key, true)?;
            job = skwd_wall_core::lock(&ctx.drawing.job);
            job.status.enabled = true;
        }
        job.count();
        job.publish(ctx);
    }
}

fn prepare_item(ctx: &Ctx, key: &str, pack: &Pack, cancel: &AtomicBool) -> Result<()> {
    let source = super::source(ctx, key)?;
    let multiple = {
        let job = skwd_wall_core::lock(&ctx.drawing.job);
        job.library || job.status.total > 1
    };
    if multiple && super::cached(ctx, key, &source).is_ok() {
        return Ok(());
    }
    execute(ctx, key, &source, pack, cancel, Instant::now())?;
    crate::infrastructure::widgets::outline_ready(&ctx.state, &source);
    Ok(())
}

fn execute(
    ctx: &Ctx,
    key: &str,
    source: &Path,
    pack: &Pack,
    cancel: &AtomicBool,
    started: Instant,
) -> Result<()> {
    update(ctx, Progress { stage: stage::CHECKING.to_string(), progress: 0.0 }, started);
    let config = ctx.state.config().clone();
    let signature = if key.starts_with(wall_proto::drawing::STATIC_KEY_PREFIX) {
        None
    } else {
        Some(cache::media_signature(&ctx.state, key, source)?)
    };
    let reference = signature.as_ref().map(|signature| {
        cache::references(&config).join(format!("{signature}.{REFERENCE_EXTENSION}"))
    });
    let mut cleanup = ReferenceCleanup(None);
    if let Some(reference) = &reference
        && capture_reference(ctx, key, source, reference, cancel, started)?
    {
        cleanup.0 = Some(reference.clone());
    }
    let input = reference.as_deref().unwrap_or(source);
    let source_hash = cache::hash_file(input)?;
    let modified = input.metadata()?.modified()?.duration_since(std::time::UNIX_EPOCH)?.as_nanos();
    let identity = cache::digest(
        format!(
            "{}:{source_hash}:{}:{}:{modified}",
            cache::RECIPE,
            pack.fingerprint,
            input.display()
        )
        .as_bytes(),
    );
    let working = cache::root(&config).join(cache::WORK_DIRECTORY);
    std::fs::create_dir_all(&working)?;
    warn_invalid_settings(ctx, &config);
    let limit_kib = memory_limit_kib(&config);
    let spec = Draw { pack, input, source_hash: &source_hash, working: &working, limit_kib };
    let temporary = draw_within_memory(ctx, &spec, device(&config, pack), cancel, started)?;
    let output = temporary.path().join(OUTPUT_DIRECTORY);
    ensure!(!cancel.load(Ordering::Acquire), "preparation cancelled");
    paper_control::DrawingManifest::open(&output.join(cache::MANIFEST), input)?;
    ensure!(cache::hash_file(input)? == source_hash, "wallpaper changed during preparation");
    if let Some(signature) = &signature {
        ensure!(cache::media_signature(&ctx.state, key, source)? == *signature, MEDIA_CHANGED);
    }
    std::fs::create_dir_all(cache::artifacts(&config))?;
    let target = cache::artifacts(&config).join(&identity);
    let mut artifact_cleanup = ArtifactCleanup(None);
    let reusable = if target.exists() {
        paper_control::DrawingManifest::open(&target.join(cache::MANIFEST), input).is_ok()
    } else {
        false
    };
    if !reusable {
        let replace = match target.symlink_metadata() {
            Ok(metadata) if metadata.is_dir() => true,
            Ok(_) => anyhow::bail!("drawing artifact is not a directory"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.into()),
        };
        if replace {
            std::fs::remove_dir_all(&target)?;
        }
        std::fs::rename(&output, &target)?;
        artifact_cleanup.0 = Some(target);
    }
    ensure!(!cancel.load(Ordering::Acquire), "preparation cancelled");
    let entry = cache::CacheEntry {
        artifact: identity,
        pack: pack.fingerprint.clone(),
        recipe: cache::RECIPE.to_string(),
    };
    let published = if let (Some(signature), Some(reference)) = (signature, reference) {
        cache::publish_media(
            &config,
            source,
            &cache::MediaEntry {
                cache: entry,
                signature,
                reference,
                source: Some(source.to_path_buf()),
            },
        )
    } else {
        cache::publish(&config, source, &entry)
    };
    if published.is_ok() {
        cleanup.0 = None;
        artifact_cleanup.0 = None;
    }
    published
}

struct Draw<'a> {
    pack: &'a Pack,
    input: &'a Path,
    source_hash: &'a str,
    working: &'a Path,
    limit_kib: u64,
}

fn draw_within_memory(
    ctx: &Ctx,
    spec: &Draw,
    device: Device,
    cancel: &AtomicBool,
    started: Instant,
) -> Result<tempfile::TempDir> {
    if device != Device::Auto {
        return draw(ctx, spec, device, cancel, started);
    }
    let limit_kib = spec.limit_kib;
    let size = image::image_dimensions(spec.input).ok();
    let pixels = size.map(|(width, height)| u64::from(width) * u64::from(height));
    let known = skwd_wall_core::lock(&ctx.drawing.job).cpu_from;
    if let (Some(pixels), Some(known)) = (pixels, known)
        && known.limit_kib == limit_kib
        && pixels >= known.pixels
    {
        return draw(ctx, spec, Device::Cpu, cancel, started);
    }
    match draw(ctx, spec, device, cancel, started) {
        Err(error) if error.is::<MemoryExceeded>() => {
            let size =
                size.map_or_else(String::new, |(width, height)| format!("{width}x{height} "));
            log::warn!("drawing: {size}exceeded the memory limit, retrying on cpu");
            skwd_wall_core::lock(&ctx.drawing.job).cpu_from =
                pixels.map(|pixels| CpuFrom { pixels, limit_kib });
            draw(ctx, spec, Device::Cpu, cancel, Instant::now())
        }
        result => result,
    }
}

fn draw(
    ctx: &Ctx,
    spec: &Draw,
    device: Device,
    cancel: &AtomicBool,
    started: Instant,
) -> Result<tempfile::TempDir> {
    let temporary = tempfile::Builder::new().prefix(JOB_PREFIX).tempdir_in(spec.working)?;
    let error_log = temporary.path().join(ERROR_LOG);
    let mut command = command(ctx, spec.pack, temporary.path(), device)?;
    command
        .arg(arg::INPUT)
        .arg(spec.input)
        .arg(arg::OUTPUT)
        .arg(temporary.path().join(OUTPUT_DIRECTORY))
        .arg(arg::SOURCE_HASH)
        .arg(spec.source_hash);
    ensure!(!cancel.load(Ordering::Acquire), "preparation cancelled");
    let child = command.spawn().context("start drawing preparation")?;
    supervise(ctx, child, spec.limit_kib, cancel, started)
        .with_context(|| error_tail(&error_log))?;
    Ok(temporary)
}

fn capture_reference(
    ctx: &Ctx,
    key: &str,
    source: &Path,
    reference: &Path,
    cancel: &AtomicBool,
    started: Instant,
) -> Result<bool> {
    if reference.is_file()
        && image::ImageReader::open(reference)
            .and_then(image::ImageReader::with_guessed_format)
            .is_ok_and(|reader| reader.decode().is_ok())
    {
        return Ok(false);
    }
    std::fs::create_dir_all(reference.parent().context("drawing reference directory missing")?)?;
    let temporary = tempfile::Builder::new()
        .suffix(&format!(".{REFERENCE_EXTENSION}"))
        .tempfile_in(reference.parent().context("drawing reference directory missing")?)?;
    let properties = key
        .strip_prefix(wall_proto::drawing::WE_KEY_PREFIX)
        .map(|we_id| skwd_wall_core::we::drawing_properties(&ctx.state, we_id))
        .unwrap_or_default();
    let request = paper_control::SceneThumbnailRequest {
        source: source.to_string_lossy().into_owned(),
        destination: temporary.path().to_string_lossy().into_owned(),
        reference: true,
        properties,
    };
    let mut child = skwd_wall_core::proc::tool(ctx.state.config().renderer().paper_bin())
        .arg(CAPTURE_ARG)
        .env(GPU_DEVICE_ENV, ctx.state.config().renderer().gpu_device())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("start drawing reference capture")?;
    let mut stdin = child.stdin.take().context("drawing reference input missing")?;
    serde_json::to_writer(&mut stdin, &request)?;
    writeln!(stdin)?;
    drop(stdin);
    loop {
        if cancel.load(Ordering::Acquire) || started.elapsed() >= TIME_LIMIT {
            let _ = child.kill();
            child.wait()?;
            anyhow::bail!("drawing reference capture cancelled or timed out");
        }
        if let Some(status) = child.try_wait()? {
            ensure!(status.success(), CAPTURE_FAILED);
            break;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    let mut response = String::new();
    child
        .stdout
        .take()
        .context("drawing reference output missing")?
        .take(MAX_PROGRESS_LINE as u64)
        .read_to_string(&mut response)?;
    let response: paper_control::SceneThumbnailResponse = serde_json::from_str(&response)?;
    ensure!(response.source == request.source, CAPTURE_SOURCE_MISMATCH);
    if let Some(error) = response.error {
        anyhow::bail!(error);
    }
    ensure!(temporary.path().metadata()?.len() > 0, CAPTURE_EMPTY);
    ensure!(
        image::ImageReader::open(temporary.path())
            .and_then(image::ImageReader::with_guessed_format)
            .is_ok_and(|reader| reader.decode().is_ok()),
        CAPTURE_INVALID
    );
    temporary.persist(reference)?;
    Ok(true)
}

fn supervise(
    ctx: &Ctx,
    mut child: Child,
    limit_kib: u64,
    cancel: &AtomicBool,
    started: Instant,
) -> Result<()> {
    let stdout = child.stdout.take().context("drawing progress stream is missing")?;
    let (sender, receiver) = mpsc::sync_channel(1);
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if line.len() > MAX_PROGRESS_LINE || sender.send(line).is_err() {
                break;
            }
        }
    });
    let result = monitor(ctx, &mut child, &receiver, limit_kib, cancel, started);
    if result.is_err() {
        let _ = child.kill();
    }
    let exit = child.wait();
    drop(receiver);
    let _ = reader.join();
    result?;
    ensure!(exit?.success(), "drawing preparation failed");
    Ok(())
}

fn monitor(
    ctx: &Ctx,
    child: &mut Child,
    receiver: &mpsc::Receiver<String>,
    limit_kib: u64,
    cancel: &AtomicBool,
    started: Instant,
) -> Result<()> {
    loop {
        ensure!(!cancel.load(Ordering::Acquire), "preparation cancelled");
        ensure!(started.elapsed() < TIME_LIMIT, "drawing preparation timed out");
        ensure!(super::can_prepare(&ctx.state.config()), "preparation stopped on battery power");
        if let Ok(status) = std::fs::read_to_string(format!("/proc/{}/status", child.id())) {
            let rss = status
                .lines()
                .find_map(|line| line.strip_prefix(RSS_PREFIX))
                .and_then(|line| line.split_whitespace().next())
                .and_then(|number| number.parse::<u64>().ok())
                .unwrap_or(0);
            if rss > limit_kib {
                return Err(MemoryExceeded.into());
            }
        }
        if let Some(status) = child.try_wait()? {
            ensure!(status.success(), "drawing preparation failed");
            return Ok(());
        }
        match receiver.recv_timeout(POLL_INTERVAL) {
            Ok(line) => {
                let progress: Progress =
                    serde_json::from_str(&line).context("invalid drawing progress")?;
                ensure!(
                    progress.progress.is_finite() && (0.0..=1.0).contains(&progress.progress),
                    "invalid drawing progress range"
                );
                update(ctx, progress, started);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                std::thread::sleep(POLL_INTERVAL);
            }
        }
    }
}

fn update(ctx: &Ctx, progress: Progress, started: Instant) {
    let mut job = skwd_wall_core::lock(&ctx.drawing.job);
    job.status.stage = progress.stage;
    job.status.progress = progress.progress;
    job.status.elapsed_ms = started.elapsed().as_millis() as u64;
    super::publish(ctx, &job.status);
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
