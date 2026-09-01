use std::collections::{BTreeSet, HashMap};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

#[cfg(not(test))]
use std::io::Write;
#[cfg(not(test))]
use std::process::Stdio;

use anyhow::{Context, bail};
use image::imageops::FilterType;
use image::{AnimationDecoder, GenericImageView, ImageDecoder};
use serde::{Deserialize, Serialize};
use skwd_wall_core::WallState;
use skwd_wall_core::infrastructure::config::ConfigStore;
use skwd_wall_core::infrastructure::database::Database;
use wall_proto::ev;

use crate::backend::events::EventPublisher;

const IMAGE_EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "gif"];
const QUALITY_SAMPLE_EDGE: u32 = 384;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Profile {
    quality: f32,
    lossless: bool,
    min_savings_percent: u8,
    min_psnr_db: f64,
    max_width: u32,
    max_height: u32,
}

impl Profile {
    fn read(config: &skwd_wall_core::config::Config) -> Self {
        let (quality, lossless, min_savings_percent, min_psnr_db) =
            match config.image_optimize_preset().as_str() {
                "light" => (82.0, false, 15, 30.0),
                // Pixel-lossless after the downscale, so PSNR is never consulted.
                "quality" => (100.0, true, 5, f64::INFINITY),
                _ => (88.0, false, 10, 32.0),
            };
        let (max_width, max_height) = match config.image_optimize_resolution().as_str() {
            "1080p" => (1920, 1080),
            "4k" => (3840, 2160),
            _ => (2560, 1440),
        };
        Self { quality, lossless, min_savings_percent, min_psnr_db, max_width, max_height }
    }

    fn key(config: &skwd_wall_core::config::Config) -> String {
        format!("v2:{}@{}", config.image_optimize_preset(), config.image_optimize_resolution())
    }

    fn bounds_for(self, width: u32, height: u32) -> (u32, u32) {
        if height > width {
            (self.max_height, self.max_width)
        } else {
            (self.max_width, self.max_height)
        }
    }

    fn saves_enough(self, original: u64, candidate: u64) -> bool {
        original > 0
            && candidate < original
            && original.saturating_sub(candidate).saturating_mul(100)
                >= original.saturating_mul(u64::from(self.min_savings_percent))
    }
}

struct Options {
    wallpaper_dir: PathBuf,
    trash_dir: PathBuf,
    legacy_trash_dir: PathBuf,
    work_dir: PathBuf,
    profile: Profile,
    profile_key: String,
    retention_days: u32,
    clean_trash: bool,
}

impl Options {
    fn read(config: &skwd_wall_core::config::Config) -> Self {
        let wallpaper_dir = PathBuf::from(config.wallpaper_dir());
        let internal = wallpaper_dir.join(skwd_wall_core::paths::INTERNAL_LIBRARY_DIR);
        Self {
            wallpaper_dir,
            trash_dir: internal.join("trash/images"),
            legacy_trash_dir: PathBuf::from(config.cache_dir()).join("wallpaper/trash/images"),
            work_dir: internal.join("work/images"),
            profile: Profile::read(config),
            profile_key: Profile::key(config),
            retention_days: config.image_trash_days(),
            clean_trash: config.image_auto_delete_trash(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct OptimizationStatus {
    pub running: bool,
    pub candidates: usize,
    pub optimized: usize,
    pub skipped_quality: usize,
    pub skipped_savings: usize,
    pub skipped_other: usize,
    pub errors: usize,
    pub original_bytes: u64,
    pub optimized_bytes: u64,
    pub trash_deleted_files: usize,
    pub trash_deleted_bytes: u64,
    pub optimized_paths: Vec<String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct PendingRequest {
    full_scan: bool,
    force: bool,
    clean_trash: bool,
    paths: BTreeSet<PathBuf>,
}

#[derive(Debug, Deserialize, Serialize)]
struct RenameEvent {
    old_name: String,
    new_name: String,
    filesize: u64,
    mtime: i64,
    width: u32,
    height: u32,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct WorkerResult {
    status: OptimizationStatus,
    renames: Vec<RenameEvent>,
    scan_paths: Vec<String>,
}

impl PendingRequest {
    fn has_work(&self) -> bool {
        self.full_scan || self.clean_trash || !self.paths.is_empty()
    }
}

impl OptimizationStatus {
    fn future_saved_bytes(&self) -> u64 {
        self.original_bytes.saturating_sub(self.optimized_bytes)
    }

    pub(super) fn to_value(&self) -> serde_json::Value {
        let mut value = serde_json::to_value(self).unwrap_or_default();
        value["future_saved_bytes"] = serde_json::json!(self.future_saved_bytes());
        value["temporary_overhead_bytes"] = serde_json::json!(self.optimized_bytes);
        value
    }
}

pub(super) struct ImageOptimizer {
    state: Arc<WallState>,
    config: Arc<ConfigStore>,
    publisher: Arc<dyn EventPublisher>,
    debug: bool,
    running: AtomicBool,
    status: Mutex<OptimizationStatus>,
    pending: Mutex<PendingRequest>,
    work_gate: Arc<Mutex<()>>,
}

impl ImageOptimizer {
    pub(super) fn new(
        state: Arc<WallState>,
        config: Arc<ConfigStore>,
        publisher: Arc<dyn EventPublisher>,
        work_gate: Arc<Mutex<()>>,
        debug: bool,
    ) -> Self {
        Self {
            state,
            config,
            publisher,
            debug,
            running: AtomicBool::new(false),
            status: Mutex::new(OptimizationStatus::default()),
            pending: Mutex::new(PendingRequest::default()),
            work_gate,
        }
    }

    pub(super) fn status(&self) -> OptimizationStatus {
        skwd_wall_core::lock(&self.status).clone()
    }

    pub(super) fn kick(self: &Arc<Self>, automatic: bool, changed_paths: &[String]) -> bool {
        self.config.reload_if_changed();
        let should_optimize = !automatic || self.config.read().image_auto_optimize();
        let should_clean = self.config.read().image_auto_delete_trash();
        if !should_optimize && !should_clean {
            return false;
        }
        {
            let mut pending = skwd_wall_core::lock(&self.pending);
            pending.clean_trash |= should_clean;
            if should_optimize {
                pending.force |= !automatic;
                if !automatic || changed_paths.is_empty() {
                    pending.full_scan = true;
                    pending.paths.clear();
                } else if !pending.full_scan {
                    pending.paths.extend(changed_paths.iter().map(PathBuf::from));
                }
            }
        }
        if self.running.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err()
        {
            return true;
        }
        *skwd_wall_core::lock(&self.status) =
            OptimizationStatus { running: true, ..OptimizationStatus::default() };
        let optimizer = Arc::clone(self);
        crate::infrastructure::proc::runtime().spawn(async move {
            loop {
                let request = {
                    let mut pending = skwd_wall_core::lock(&optimizer.pending);
                    std::mem::take(&mut *pending)
                };
                if request.has_work() {
                    let batch = Arc::clone(&optimizer);
                    let _ = tokio::task::spawn_blocking(move || batch.run_batch(&request)).await;
                }
                {
                    let pending = skwd_wall_core::lock(&optimizer.pending);
                    if !pending.has_work() {
                        optimizer.running.store(false, Ordering::Release);
                        break;
                    }
                }
            }
        });
        true
    }

    fn run_batch(&self, request: &PendingRequest) {
        let _permit = skwd_wall_core::lock(&self.work_gate);
        skwd_wall_core::lock(&self.status).running = true;
        let result = run_worker_process(request).unwrap_or_else(|error| {
            log::warn!("image optimize worker failed: {error:#}");
            WorkerResult {
                status: OptimizationStatus { errors: 1, ..OptimizationStatus::default() },
                ..WorkerResult::default()
            }
        });
        let status = result.status;
        for rename in result.renames {
            self.publisher.publish(
                ev::FILE_RENAMED,
                serde_json::json!({
                    "old_name": rename.old_name,
                    "new_name": rename.new_name,
                    "filesize": rename.filesize,
                    "mtime": rename.mtime,
                    "width": rename.width,
                    "height": rename.height,
                }),
            );
        }
        log::info!(
            "image optimize: {} optimized, {} errors, {} bytes saved after retention ({} bytes temporary overhead)",
            status.optimized,
            status.errors,
            status.future_saved_bytes(),
            status.optimized_bytes
        );
        *skwd_wall_core::lock(&self.status) = status.clone();
        self.publisher.publish(ev::IMAGE_OPTIMIZE_COMPLETE, status.to_value());
        if !result.scan_paths.is_empty() {
            let mut arguments = Vec::with_capacity(result.scan_paths.len() + 1);
            arguments.push("--paths");
            arguments.extend(result.scan_paths.iter().map(String::as_str));
            super::processes::spawn_scan(&self.state, self.debug, &arguments, None, None);
        }
    }
}

fn run_worker_process(request: &PendingRequest) -> anyhow::Result<WorkerResult> {
    #[cfg(test)]
    {
        let mut result = run_worker_request(request)?;
        result.scan_paths.clear();
        Ok(result)
    }
    #[cfg(not(test))]
    {
        let executable = std::env::current_exe().context("resolve image optimize worker")?;
        let mut child = skwd_wall_core::proc::tool(executable)
            .arg("--image-optimize-worker")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .context("spawn image optimize worker")?;
        let payload = serde_json::to_vec(request)?;
        child.stdin.take().context("image optimize worker stdin")?.write_all(&payload)?;
        let output = child.wait_with_output().context("wait for image optimize worker")?;
        anyhow::ensure!(
            output.status.success(),
            "image optimize worker exited with {}",
            output.status
        );
        serde_json::from_slice(&output.stdout).context("decode image optimize worker result")
    }
}

pub(crate) fn run_worker() -> anyhow::Result<()> {
    let mut payload = Vec::new();
    std::io::stdin().read_to_end(&mut payload)?;
    let request: PendingRequest = serde_json::from_slice(&payload)?;
    let result = run_worker_request(&request)?;
    serde_json::to_writer(std::io::stdout().lock(), &result)?;
    Ok(())
}

fn run_worker_request(request: &PendingRequest) -> anyhow::Result<WorkerResult> {
    let state = WallState::open().context("open image optimize worker state")?;
    let options = Options::read(&state.config());
    let database = state.database();
    Ok(run_worker_batch(&options, &database, request))
}

fn run_worker_batch(
    options: &Options,
    database: &Database,
    request: &PendingRequest,
) -> WorkerResult {
    clean_work_dir(&options.work_dir);
    let mut result = WorkerResult::default();
    result.status.running = true;
    if request.clean_trash && options.clean_trash {
        let mut deleted = clean_trash(&options.trash_dir, options.retention_days);
        let legacy = clean_trash(&options.legacy_trash_dir, options.retention_days);
        deleted.files += legacy.files;
        deleted.bytes = deleted.bytes.saturating_add(legacy.bytes);
        result.status.trash_deleted_files = deleted.files;
        result.status.trash_deleted_bytes = deleted.bytes;
    }
    if request.full_scan || !request.paths.is_empty() {
        let requested = (!request.full_scan).then_some(&request.paths);
        result.status = optimize_library(
            options,
            database,
            request.force,
            requested,
            result.status,
            &mut result.renames,
            &mut result.scan_paths,
        );
    }
    result.status.running = false;
    result
}

fn optimize_library(
    options: &Options,
    database: &Database,
    force: bool,
    requested: Option<&BTreeSet<PathBuf>>,
    mut status: OptimizationStatus,
    renames: &mut Vec<RenameEvent>,
    scan_paths: &mut Vec<String>,
) -> OptimizationStatus {
    let prior: HashMap<String, (String, u64, i64)> = database
        .with_connection(skwd_wall_core::db::image_optimization_records)
        .unwrap_or_default()
        .into_iter()
        .map(|(source, profile, size, optimized_at)| (source, (profile, size, optimized_at)))
        .collect();
    let sources = requested.map_or_else(
        || image_sources(&options.wallpaper_dir),
        |paths| image_sources_from(&options.wallpaper_dir, paths),
    );
    status.candidates = sources.len();
    log::info!("image optimize: {} candidate(s), profile {}", sources.len(), options.profile_key);
    for source in sources {
        let source_text = source.to_string_lossy().into_owned();
        if !force
            && let Some((profile, size, optimized_at)) = prior.get(&source_text)
            && profile == &options.profile_key
            && source_unchanged(&source, *size, *optimized_at)
        {
            continue;
        }
        match optimize_one(&source, options) {
            Ok(Optimized::Changed(result)) => {
                let stored = database.with_connection(|connection| {
                    skwd_wall_core::db::record_image_optimization_and_rename(
                        connection,
                        &source_text,
                        &result.destination.to_string_lossy(),
                        &options.profile_key,
                        "webp",
                        result.width,
                        result.height,
                        result.original_size,
                        result.new_size,
                        result.mtime,
                        &format!("static:{}", result.old_name),
                        &format!("static:{}", result.new_name),
                        &result.new_name,
                    )
                });
                if let Err(error) = stored {
                    status.errors += 1;
                    rollback_install(&result);
                    log::warn!(
                        "image optimize: database update failed for {}, kept original: {error}",
                        source.display()
                    );
                    continue;
                }
                if let Err(error) = move_file(&result.source, &result.trash) {
                    status.errors += 1;
                    let rolled_back = database.with_connection(|connection| {
                        skwd_wall_core::db::rollback_image_optimization_and_rename(
                            connection,
                            &source_text,
                            &format!("static:{}", result.old_name),
                            &format!("static:{}", result.new_name),
                            &result.old_name,
                        )
                    });
                    if rolled_back.is_ok() {
                        rollback_install(&result);
                        log::warn!(
                            "image optimize: could not move {} to recovery ({error}); rolled back",
                            source.display()
                        );
                    } else {
                        log::error!(
                            "image optimize: could not move {} to recovery ({error}) and database rollback failed; both complete files were preserved",
                            source.display()
                        );
                    }
                    continue;
                }
                touch_modified_now(&result.trash);
                crate::infrastructure::persistence::repoint_optimized_image(
                    &source_text,
                    &result.destination.to_string_lossy(),
                );
                renames.push(RenameEvent {
                    old_name: result.old_name.clone(),
                    new_name: result.new_name.clone(),
                    filesize: result.new_size,
                    mtime: result.mtime,
                    width: result.width,
                    height: result.height,
                });
                status.optimized += 1;
                status.original_bytes = status.original_bytes.saturating_add(result.original_size);
                status.optimized_bytes = status.optimized_bytes.saturating_add(result.new_size);
                status.optimized_paths.push(result.destination.to_string_lossy().into_owned());
                if !image_artifacts_ready(&result.new_name) {
                    scan_paths.push(result.destination.to_string_lossy().into_owned());
                }
                log::info!(
                    "image optimize: {} -> {} ({} -> {} bytes)",
                    source.display(),
                    result.destination.display(),
                    result.original_size,
                    result.new_size
                );
            }
            Ok(Optimized::Skipped(reason)) => {
                match reason {
                    "quality-gate" | "icc-profile" => status.skipped_quality += 1,
                    "not-enough-savings" | "not-smaller" => status.skipped_savings += 1,
                    _ => status.skipped_other += 1,
                }
                let size = std::fs::metadata(&source).map_or(0, |metadata| metadata.len());
                let _ = database.with_connection(|connection| {
                    skwd_wall_core::db::image_optimization_record(
                        connection,
                        &source_text,
                        &source_text,
                        &options.profile_key,
                        reason,
                        0,
                        0,
                        size,
                        size,
                    )
                });
                log::debug!("image optimize: skipped {} ({reason})", source.display());
            }
            Err(error) => {
                status.errors += 1;
                let size = std::fs::metadata(&source).map_or(0, |metadata| metadata.len());
                let _ = database.with_connection(|connection| {
                    skwd_wall_core::db::image_optimization_record(
                        connection,
                        &source_text,
                        &source_text,
                        &options.profile_key,
                        "error",
                        0,
                        0,
                        size,
                        size,
                    )
                });
                log::warn!("image optimize: {} failed: {error:#}", source.display());
            }
        }
    }
    status
}

fn source_unchanged(source: &Path, recorded_size: u64, optimized_at: i64) -> bool {
    let Ok(metadata) = std::fs::metadata(source) else {
        return false;
    };
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map_or(i64::MAX, |duration| i64::try_from(duration.as_secs()).unwrap_or(i64::MAX));
    metadata.len() == recorded_size && modified_at <= optimized_at
}

fn modified_secs(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map_or(0, |duration| i64::try_from(duration.as_secs()).unwrap_or(i64::MAX))
}

fn image_artifacts_ready(name: &str) -> bool {
    let thumb = skwd_wall_core::paths::image_thumb(name);
    let small = skwd_wall_core::paths::image_thumb_sm(name);
    let base = thumb.to_string_lossy().replace("/thumbs/", "/blocks/");
    let base = base.strip_suffix(".webp").unwrap_or(&base);
    thumb.is_file()
        && small.is_file()
        && Path::new(&format!("{base}.bc7")).is_file()
        && Path::new(&format!("{base}.bc1")).is_file()
}

fn image_sources(root: &Path) -> Vec<PathBuf> {
    let mut sources: Vec<PathBuf> = walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !skwd_wall_core::paths::is_internal_library_path(entry.path()))
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(walkdir::DirEntry::into_path)
        .filter(|path| {
            path.extension()
                .and_then(std::ffi::OsStr::to_str)
                .map(str::to_ascii_lowercase)
                .is_some_and(|extension| IMAGE_EXTENSIONS.contains(&extension.as_str()))
        })
        .collect();
    sources.sort();
    sources
}

fn image_sources_from(root: &Path, paths: &BTreeSet<PathBuf>) -> Vec<PathBuf> {
    paths
        .iter()
        .filter(|path| path.is_file())
        .filter(|path| path.starts_with(root))
        .filter(|path| !skwd_wall_core::paths::is_internal_library_path(path))
        .filter(|path| {
            path.extension()
                .and_then(std::ffi::OsStr::to_str)
                .map(str::to_ascii_lowercase)
                .is_some_and(|extension| IMAGE_EXTENSIONS.contains(&extension.as_str()))
        })
        .cloned()
        .collect()
}

struct OptimizeResult {
    source: PathBuf,
    destination: PathBuf,
    trash: PathBuf,
    old_name: String,
    new_name: String,
    original_size: u64,
    new_size: u64,
    mtime: i64,
    width: u32,
    height: u32,
}

enum Optimized {
    Changed(OptimizeResult),
    Skipped(&'static str),
}

fn optimize_one(source: &Path, options: &Options) -> anyhow::Result<Optimized> {
    let relative = source
        .strip_prefix(&options.wallpaper_dir)
        .with_context(|| format!("{} is outside wallpaper library", source.display()))?;
    let destination = source.with_extension("webp");
    if destination.exists() {
        return Ok(Optimized::Skipped("target-exists"));
    }
    let temporary = temporary_path(&options.work_dir, relative);
    let extension = source
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let encode = if extension == "gif" && gif_is_animated(source)? {
        Ok(EncodeAttempt::Skipped("animated-source"))
    } else {
        encode_static(source, &temporary, options.profile)
    };
    let (width, height) = match encode {
        Ok(EncodeAttempt::Written { width, height }) => (width, height),
        Ok(EncodeAttempt::Skipped(reason)) => {
            let _ = std::fs::remove_file(&temporary);
            return Ok(Optimized::Skipped(reason));
        }
        Err(error) => {
            let _ = std::fs::remove_file(&temporary);
            return Err(error);
        }
    };
    let original_size = std::fs::metadata(source)?.len();
    let new_size = std::fs::metadata(&temporary)?.len();
    if new_size == 0 || new_size >= original_size {
        let _ = std::fs::remove_file(&temporary);
        return Ok(Optimized::Skipped(if new_size == 0 { "empty" } else { "not-smaller" }));
    }
    if !options.profile.saves_enough(original_size, new_size) {
        let _ = std::fs::remove_file(&temporary);
        return Ok(Optimized::Skipped("not-enough-savings"));
    }

    let trash = unique_trash_path(&options.trash_dir, relative);
    if let Err(error) = std::fs::rename(&temporary, &destination) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error).context("install optimized image");
    }

    let old_name = relative.to_string_lossy().into_owned();
    let new_name = relative.with_extension("webp").to_string_lossy().into_owned();
    let mtime = modified_secs(&destination);
    Ok(Optimized::Changed(OptimizeResult {
        source: source.to_path_buf(),
        destination,
        trash,
        old_name,
        new_name,
        original_size,
        new_size,
        mtime,
        width,
        height,
    }))
}

enum EncodeAttempt {
    Written { width: u32, height: u32 },
    Skipped(&'static str),
}

fn rollback_install(result: &OptimizeResult) {
    if result.destination.exists()
        && let Err(error) = std::fs::remove_file(&result.destination)
    {
        log::error!("image optimize: could not remove rolled-back output: {error}");
    }
}

fn encode_static(
    source: &Path,
    destination: &Path,
    profile: Profile,
) -> anyhow::Result<EncodeAttempt> {
    let mut reader = image::ImageReader::open(source)?.with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(skwd_wall_core::domain::media_limits::IMAGE_MAX_EDGE);
    limits.max_image_height = Some(skwd_wall_core::domain::media_limits::IMAGE_MAX_EDGE);
    limits.max_alloc = Some(skwd_wall_core::domain::media_limits::IMAGE_MAX_DECODE_ALLOC);
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    if decoder.icc_profile()?.is_some() {
        return Ok(EncodeAttempt::Skipped("icc-profile"));
    }
    let orientation = decoder.orientation()?;
    let mut decoded = image::DynamicImage::from_decoder(decoder)
        .with_context(|| format!("decode {}", source.display()))?;
    decoded.apply_orientation(orientation);
    let bounds = profile.bounds_for(decoded.width(), decoded.height());
    if decoded.width() > bounds.0 || decoded.height() > bounds.1 {
        decoded = decoded.resize(bounds.0, bounds.1, FilterType::Lanczos3);
    }
    let (width, height) = (decoded.width(), decoded.height());
    let samples = PixelSamples::collect(&decoded);
    let prepared = PreparedWebp::new(&decoded, profile)?;
    drop(decoded);
    prepared.encode_to(destination)?;
    let roundtrip = decode_for_validation(destination)?;
    if (roundtrip.width(), roundtrip.height()) != (width, height) {
        bail!(
            "optimized dimensions changed from {width}x{height} to {}x{}",
            roundtrip.width(),
            roundtrip.height()
        );
    }
    if !profile.lossless {
        let psnr = samples.psnr(&roundtrip);
        if psnr.is_nan() || psnr < profile.min_psnr_db {
            log::info!(
                "image optimize: quality gate rejected {} ({psnr:.2} dB < {:.2} dB)",
                source.display(),
                profile.min_psnr_db
            );
            return Ok(EncodeAttempt::Skipped("quality-gate"));
        }
    }
    Ok(EncodeAttempt::Written { width, height })
}

fn decode_for_validation(path: &Path) -> anyhow::Result<image::DynamicImage> {
    let mut reader = image::ImageReader::open(path)?.with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(skwd_wall_core::domain::media_limits::IMAGE_MAX_EDGE);
    limits.max_image_height = Some(skwd_wall_core::domain::media_limits::IMAGE_MAX_EDGE);
    limits.max_alloc = Some(skwd_wall_core::domain::media_limits::IMAGE_MAX_DECODE_ALLOC);
    reader.limits(limits);
    image::DynamicImage::from_decoder(reader.into_decoder()?)
        .with_context(|| format!("validate {}", path.display()))
}

struct PixelSamples {
    width: u32,
    height: u32,
    sample_width: u32,
    sample_height: u32,
    pixels: Vec<[u8; 4]>,
}

impl PixelSamples {
    fn collect(image: &image::DynamicImage) -> Self {
        let (width, height) = image.dimensions();
        let sample_width = width.min(QUALITY_SAMPLE_EDGE);
        let sample_height = height.min(QUALITY_SAMPLE_EDGE);
        let mut pixels = Vec::with_capacity(sample_width as usize * sample_height as usize);
        for sample_y in 0..sample_height {
            let y = u64::from(sample_y) * u64::from(height.saturating_sub(1))
                / u64::from(sample_height.saturating_sub(1).max(1));
            for sample_x in 0..sample_width {
                let x = u64::from(sample_x) * u64::from(width.saturating_sub(1))
                    / u64::from(sample_width.saturating_sub(1).max(1));
                pixels.push(image.get_pixel(x as u32, y as u32).0);
            }
        }
        Self { width, height, sample_width, sample_height, pixels }
    }

    fn psnr(&self, candidate: &image::DynamicImage) -> f64 {
        if candidate.dimensions() != (self.width, self.height) || self.pixels.is_empty() {
            return f64::NEG_INFINITY;
        }
        let mut squared_error = 0.0;
        let mut index = 0usize;
        for sample_y in 0..self.sample_height {
            let y = u64::from(sample_y) * u64::from(self.height.saturating_sub(1))
                / u64::from(self.sample_height.saturating_sub(1).max(1));
            for sample_x in 0..self.sample_width {
                let x = u64::from(sample_x) * u64::from(self.width.saturating_sub(1))
                    / u64::from(self.sample_width.saturating_sub(1).max(1));
                let left = self.pixels[index];
                let right = candidate.get_pixel(x as u32, y as u32).0;
                for channel in 0..4 {
                    let delta = f64::from(left[channel]) - f64::from(right[channel]);
                    squared_error += delta * delta;
                }
                index += 1;
            }
        }
        let mean = squared_error / (self.pixels.len() * 4) as f64;
        if mean == 0.0 { f64::INFINITY } else { 10.0 * (255.0f64.powi(2) / mean).log10() }
    }
}

struct PreparedWebp {
    config: libwebp_sys::WebPConfig,
    picture: libwebp_sys::WebPPicture,
}

impl PreparedWebp {
    fn new(image: &image::DynamicImage, profile: Profile) -> anyhow::Result<Self> {
        use std::borrow::Cow;

        let (width, height) = image.dimensions();
        let (pixels, stride, has_alpha): (Cow<'_, [u8]>, i32, bool) = if image.color().has_alpha() {
            match image.as_rgba8() {
                Some(rgba) => (Cow::Borrowed(rgba.as_raw().as_slice()), width as i32 * 4, true),
                None => (Cow::Owned(image.to_rgba8().into_raw()), width as i32 * 4, true),
            }
        } else {
            match image.as_rgb8() {
                Some(rgb) => (Cow::Borrowed(rgb.as_raw().as_slice()), width as i32 * 3, false),
                None => (Cow::Owned(image.to_rgb8().into_raw()), width as i32 * 3, false),
            }
        };
        unsafe {
            let mut config = libwebp_sys::WebPConfig::new()
                .map_err(|()| anyhow::anyhow!("WebP config initialization failed"))?;
            config.lossless = i32::from(profile.lossless);
            config.quality = profile.quality;
            config.alpha_compression = 1;
            config.low_memory = 1;
            config.thread_level = 0;
            config.exact = i32::from(profile.lossless);
            anyhow::ensure!(libwebp_sys::WebPValidateConfig(&config) != 0, "invalid WebP config");
            let mut picture = libwebp_sys::WebPPicture::new()
                .map_err(|()| anyhow::anyhow!("WebP picture initialization failed"))?;
            picture.use_argb = i32::from(profile.lossless);
            picture.width = width as i32;
            picture.height = height as i32;
            let imported = if has_alpha {
                libwebp_sys::WebPPictureImportRGBA(&mut picture, pixels.as_ptr(), stride)
            } else {
                libwebp_sys::WebPPictureImportRGB(&mut picture, pixels.as_ptr(), stride)
            };
            if imported == 0 {
                libwebp_sys::WebPPictureFree(&mut picture);
                bail!("WebP pixel import failed");
            }
            Ok(Self { config, picture })
        }
    }

    fn encode_to(mut self, destination: &Path) -> anyhow::Result<()> {
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        unsafe {
            let mut writer: libwebp_sys::WebPMemoryWriter = std::mem::zeroed();
            libwebp_sys::WebPMemoryWriterInit(&mut writer);
            self.picture.writer = Some(libwebp_sys::WebPMemoryWrite);
            self.picture.custom_ptr = (&raw mut writer).cast();
            if libwebp_sys::WebPEncode(&self.config, &mut self.picture) == 0 {
                let error = self.picture.error_code;
                libwebp_sys::WebPMemoryWriterClear(&mut writer);
                bail!("WebP encode failed: {error:?}");
            }
            if writer.mem.is_null() || writer.size == 0 {
                libwebp_sys::WebPMemoryWriterClear(&mut writer);
                bail!("WebP encoder returned an empty result");
            }
            let encoded = std::slice::from_raw_parts(writer.mem, writer.size);
            let result = std::fs::write(destination, encoded)
                .with_context(|| format!("write {}", destination.display()));
            libwebp_sys::WebPMemoryWriterClear(&mut writer);
            result?;
        }
        Ok(())
    }
}

impl Drop for PreparedWebp {
    fn drop(&mut self) {
        unsafe {
            libwebp_sys::WebPPictureFree(&mut self.picture);
        }
    }
}

fn gif_is_animated(path: &Path) -> anyhow::Result<bool> {
    let file = std::fs::File::open(path)?;
    let mut decoder = image::codecs::gif::GifDecoder::new(BufReader::new(file))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(skwd_wall_core::domain::media_limits::IMAGE_MAX_EDGE);
    limits.max_image_height = Some(skwd_wall_core::domain::media_limits::IMAGE_MAX_EDGE);
    limits.max_alloc = Some(skwd_wall_core::domain::media_limits::IMAGE_MAX_DECODE_ALLOC);
    decoder.set_limits(limits)?;
    Ok(decoder.into_frames().take(2).count() > 1)
}

fn temporary_path(work_root: &Path, relative: &Path) -> PathBuf {
    let parent = relative.parent().unwrap_or_else(|| Path::new(""));
    let name = relative.file_name().unwrap_or_default().to_string_lossy();
    work_root.join(parent).join(format!("{name}.{}.webp", skwd_wall_core::paths::tmp_suffix()))
}

fn unique_trash_path(root: &Path, relative: &Path) -> PathBuf {
    let parent = relative.parent().unwrap_or_else(|| Path::new(""));
    let directory = root.join(parent);
    let _ = std::fs::create_dir_all(&directory);
    let name = relative.file_name().unwrap_or_default().to_string_lossy();
    let base = directory.join(&*name);
    if !base.exists() {
        return base;
    }
    for index in 1..u32::MAX {
        let candidate = directory.join(format!("{name}.{index}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    directory.join(format!("{name}.{}", skwd_wall_core::paths::tmp_suffix()))
}

fn move_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(error) if error.raw_os_error() == Some(libc::EXDEV) => {
            std::fs::copy(source, destination)?;
            if let Err(remove_error) = std::fs::remove_file(source) {
                let _ = std::fs::remove_file(destination);
                return Err(remove_error);
            }
            Ok(())
        }
        Err(error) => Err(error),
    }
}

fn touch_modified_now(path: &Path) {
    if let Ok(file) = std::fs::OpenOptions::new().write(true).open(path) {
        let times = std::fs::FileTimes::new().set_modified(SystemTime::now());
        let _ = file.set_times(times);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CleanupStats {
    files: usize,
    bytes: u64,
}

fn clean_trash(root: &Path, retention_days: u32) -> CleanupStats {
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(u64::from(retention_days) * 86_400))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let mut stats = CleanupStats::default();
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .contents_first(true)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| {
            entry
                .metadata()
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .is_some_and(|modified| modified <= cutoff)
        })
    {
        let bytes = entry.metadata().map_or(0, |metadata| metadata.len());
        if std::fs::remove_file(entry.path()).is_ok() {
            stats.files += 1;
            stats.bytes = stats.bytes.saturating_add(bytes);
        }
    }
    remove_empty_directories(root);
    stats
}

fn clean_work_dir(root: &Path) {
    if root.is_dir() {
        let _ = std::fs::remove_dir_all(root);
    }
}

fn remove_empty_directories(root: &Path) {
    let mut directories: Vec<PathBuf> = walkdir::WalkDir::new(root)
        .min_depth(1)
        .contents_first(true)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_dir())
        .map(walkdir::DirEntry::into_path)
        .collect();
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for directory in directories {
        let _ = std::fs::remove_dir(directory);
    }
}

#[cfg(test)]
#[path = "image_optimization/tests.rs"]
mod tests;
