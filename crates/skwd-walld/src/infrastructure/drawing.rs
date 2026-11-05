use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use anyhow::{Result, ensure};
use skwd_wall_core::infrastructure::drawing as cache;
use wall_proto::drawing::{
    KeyRequest, MAX_PREPARE_KEYS, PackState, Phase, PrepareRequest, RegisterRequest,
    SetEnabledRequest, Status,
};
use wall_proto::{Request, Response, ev, rpc};

use crate::backend::events::EventPublisher;
use crate::composition::context::Ctx;

mod batch;
mod pack;
mod preference;
mod run;

pub(crate) use batch::{auto, outline};

const INVALID_PARAMS: i32 = -32602;
const FAILED: i32 = -32000;
const PREPARE_RANGE: &str = "choose a valid number of wallpapers";
const INVALID_WE_ID: &str = "invalid Wallpaper Engine id";
const UNSUPPORTED_WE_PROJECT: &str = "unsupported Wallpaper Engine project";
const BUSY: &str = "another drawing is being prepared";
const ON_BATTERY: &str = "preparation is paused on battery power";

#[derive(Default)]
pub(crate) struct DrawingJobs {
    control: Mutex<()>,
    job: Mutex<Job>,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Activity {
    #[default]
    Stopped,
    Waiting,
    Busy,
}

#[derive(Default)]
struct Job {
    status: Status,
    cancel: Option<Arc<AtomicBool>>,
    thread: Option<JoinHandle<()>>,
    pending: VecDeque<String>,
    enable: BTreeSet<String>,
    failed: BTreeSet<String>,
    outlines: BTreeSet<String>,
    activity: Activity,
    library: bool,
    plan: bool,
    cpu_from: Option<CpuFrom>,
    warned: BTreeSet<String>,
    system_checked: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct CpuFrom {
    pixels: u64,
    limit_kib: u64,
}

impl Job {
    fn check(&mut self, ctx: &Ctx, path: PathBuf) {
        let cancel = Arc::new(AtomicBool::new(false));
        self.status = Status {
            phase: Phase::Checking,
            pack_path: path.to_string_lossy().into_owned(),
            ..Status::default()
        };
        self.cancel = Some(Arc::clone(&cancel));
        self.system_checked = true;
        publish(ctx, &self.status);
        let ctx = ctx.clone();
        self.thread = Some(std::thread::spawn(move || {
            run::validate(&ctx, &path, &cancel);
        }));
    }

    fn idle(&self) -> bool {
        let settled = self.activity == Activity::Stopped && self.status.phase != Phase::Checking;
        settled || self.thread.as_ref().is_none_or(std::thread::JoinHandle::is_finished)
    }

    fn count(&mut self) {
        self.status.total = self.status.completed
            + self.status.failed
            + self.pending.len() as u32
            + u32::from(self.activity == Activity::Busy);
    }

    fn jump(&mut self, keys: &[String]) {
        for key in keys.iter().rev() {
            self.pending.retain(|pending| pending != key);
            self.pending.push_front(key.clone());
        }
    }

    fn publish(&self, ctx: &Ctx) {
        publish(ctx, &self.status);
        if self.library {
            batch::report(ctx, &self.status);
        }
    }

    fn spawn(&mut self, ctx: &Ctx, pack: pack::Pack) {
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Some(Arc::clone(&cancel));
        self.activity = Activity::Waiting;
        self.status.phase = Phase::Preparing;
        self.status.stage = wall_proto::drawing::stage::STARTING.to_string();
        self.status.error = None;
        self.count();
        self.publish(ctx);
        let ctx = ctx.clone();
        self.thread = Some(std::thread::spawn(move || {
            run::prepare(&ctx, &pack, &cancel);
        }));
    }
}

impl DrawingJobs {
    fn register(&self, ctx: &Ctx, path: PathBuf) {
        self.cancel(ctx, Phase::Idle);
        skwd_wall_core::lock(&self.job).check(ctx, path);
    }

    pub(crate) fn cancel(&self, ctx: &Ctx, phase: Phase) {
        let handle = {
            let mut job = skwd_wall_core::lock(&self.job);
            if let Some(cancel) = job.cancel.take() {
                cancel.store(true, Ordering::Release);
            }
            job.thread.take()
        };
        if let Some(handle) = handle {
            let _ = handle.join();
        }
        let mut job = skwd_wall_core::lock(&self.job);
        job.activity = Activity::Stopped;
        job.status.phase = phase;
        job.status.stage.clear();
        job.status.error = None;
        job.status.prepared = false;
        job.status.preview_path = None;
        if phase != Phase::Paused {
            job.pending.clear();
            job.enable.clear();
            job.outlines.clear();
        }
        job.count();
        job.publish(ctx);
        if phase != Phase::Paused {
            job.library = false;
        }
    }

    fn prepare(
        &self,
        ctx: &Ctx,
        keys: Vec<String>,
        enable: BTreeSet<String>,
        outlines: BTreeSet<String>,
    ) -> Result<()> {
        ensure!(!keys.is_empty() && keys.len() <= MAX_PREPARE_KEYS, PREPARE_RANGE);
        for key in &keys {
            source(ctx, key)?;
        }
        let config = ctx.state.config().clone();
        ensure!(can_prepare(&config), ON_BATTERY);
        let pack = pack::Pack::load(&config)?;
        let mut job = skwd_wall_core::lock(&self.job);
        if job.activity != Activity::Stopped {
            ensure!(job.library, BUSY);
            job.jump(&keys);
            job.enable.extend(enable);
            job.outlines.extend(outlines);
            job.count();
            job.publish(ctx);
            return Ok(());
        }
        ensure!(job.idle(), BUSY);
        job.status = Status {
            key: keys[0].clone(),
            active_key: keys[0].clone(),
            enabled: skwd_config::drawing_enabled(config.root(), &keys[0]),
            ..Status::default()
        };
        job.pending = keys.into();
        job.outlines = outlines;
        job.enable = enable;
        job.library = false;
        job.plan = false;
        job.spawn(ctx, pack);
        Ok(())
    }

    fn prepare_library(&self, ctx: &Ctx, retry: bool) -> Result<()> {
        let config = ctx.state.config().clone();
        ensure!(can_prepare(&config), ON_BATTERY);
        let pack = pack::Pack::load(&config)?;
        let mut job = skwd_wall_core::lock(&self.job);
        ensure!(job.activity != Activity::Stopped || job.idle(), BUSY);
        batch::hold(&config, false)?;
        if retry {
            job.failed.clear();
        }
        job.library = true;
        job.plan = true;
        if job.activity == Activity::Stopped {
            job.status = Status::default();
            job.spawn(ctx, pack);
        }
        Ok(())
    }

    fn adopt_system_pack(&self, ctx: &Ctx, config: &skwd_wall_core::config::Config) {
        {
            let mut job = skwd_wall_core::lock(&self.job);
            if job.system_checked || job.activity != Activity::Stopped || !job.idle() {
                return;
            }
            job.system_checked = true;
        }
        if let Some(path) = pack::system_manifest(config) {
            self.register(ctx, path);
        }
    }

    fn status(&self, ctx: &Ctx, key: &str) -> Status {
        let config = ctx.state.config().clone();
        self.adopt_system_pack(ctx, &config);
        let (mut status, library) = {
            let job = skwd_wall_core::lock(&self.job);
            (job.status.clone(), job.library)
        };
        if !key.is_empty() && !status.active_key.is_empty() && key != status.active_key {
            status.error = None;
            if library || !matches!(status.phase, Phase::Preparing | Phase::Paused) {
                status.phase = Phase::Idle;
                status.stage.clear();
                status.progress = 0.0;
                status.elapsed_ms = 0;
            }
        }
        status.cache_bytes = cache::directory_bytes(&cache::artifacts(&config)).unwrap_or(0)
            + cache::directory_bytes(&cache::references(&config)).unwrap_or(0);
        let failed_registration = status.phase == Phase::Failed
            && status.active_key.is_empty()
            && !status.pack_path.is_empty();
        if failed_registration {
            status.pack = PackState::Invalid;
        } else {
            match pack::Pack::load(&config) {
                Ok(pack) => {
                    status.pack = PackState::Available;
                    if status.phase != Phase::Checking {
                        status.pack_path = pack.path.to_string_lossy().into_owned();
                        status.pack_bytes = pack.bytes;
                    }
                }
                Err(error) if cache::root(&config).join(cache::PACK_RECORD).exists() => {
                    status.pack = PackState::Invalid;
                    if status.phase != Phase::Checking {
                        status.error = Some(error.to_string());
                    }
                }
                Err(_) => status.pack = PackState::Missing,
            }
        }
        if !key.is_empty() {
            status.key = key.to_string();
            status.prepared = false;
            status.preview_path = None;
            if let Ok(source) = source(ctx, key)
                && let Ok(artifact) = cached(ctx, key, &source)
            {
                status.prepared = true;
                status.preview_path = artifact
                    .parent()
                    .map(|directory| directory.join(cache::PREVIEW).to_string_lossy().into_owned());
                if status.phase == Phase::Idle {
                    status.phase = Phase::Ready;
                }
            }
        }
        status.enabled = skwd_config::drawing_enabled(config.root(), &status.key);
        status
    }
}

pub(crate) fn dispatch(ctx: &Ctx, req: &Request) -> Response {
    let _control =
        (req.method != rpc::DRAWING_STATUS).then(|| skwd_wall_core::lock(&ctx.drawing.control));
    let request = match serde_json::from_value::<KeyRequest>(req.params.clone()) {
        Ok(request) => request,
        Err(error) => return Response::err(req.id, INVALID_PARAMS, error.to_string()),
    };
    let result = match req.method.as_str() {
        rpc::DRAWING_STATUS => Ok(()),
        rpc::DRAWING_REGISTER => register(ctx, &req.params),
        rpc::DRAWING_PREPARE => prepare(ctx, &req.params),
        rpc::DRAWING_SET_ENABLED => set_enabled(ctx, &req.params),
        rpc::DRAWING_RESUME => resume(ctx),
        rpc::DRAWING_PAUSE => pause(ctx),
        rpc::DRAWING_CANCEL => stop(ctx),
        rpc::DRAWING_DETACH => {
            ctx.drawing.cancel(ctx, Phase::Idle);
            pack::detach(&ctx.state.config())
        }
        rpc::DRAWING_CLEAR_CACHE => clear(ctx),
        rpc::DRAWING_PREVIEW => preview(ctx, &request),
        _ => unreachable!(),
    };
    match result {
        Ok(()) => {
            ctx.state.reload_config();
            let status = ctx.drawing.status(ctx, &request.key);
            if req.method != rpc::DRAWING_STATUS {
                publish(ctx, &status);
            }
            Response::ok(req.id, serde_json::json!(status))
        }
        Err(error) => Response::err(req.id, FAILED, format!("{error:#}")),
    }
}

fn register(ctx: &Ctx, params: &serde_json::Value) -> Result<()> {
    let request: RegisterRequest = serde_json::from_value(params.clone())?;
    let path = PathBuf::from(request.path);
    ensure!(path.is_absolute() && path.is_file(), "choose an existing processing manifest");
    ctx.drawing.register(ctx, path);
    Ok(())
}

fn prepare(ctx: &Ctx, params: &serde_json::Value) -> Result<()> {
    let request: PrepareRequest = serde_json::from_value(params.clone())?;
    if request.library {
        return ctx.drawing.prepare_library(ctx, true);
    }
    let keys = if request.keys.is_empty() { vec![request.key] } else { request.keys };
    let mut seen = std::collections::BTreeSet::new();
    let keys = keys.into_iter().filter(|key| seen.insert(key.clone())).collect();
    ctx.drawing.prepare(ctx, keys, BTreeSet::new(), BTreeSet::new())
}

fn set_enabled(ctx: &Ctx, params: &serde_json::Value) -> Result<()> {
    let request: SetEnabledRequest = serde_json::from_value(params.clone())?;
    let source = source(ctx, &request.key)?;
    if !request.enabled {
        let mut job = skwd_wall_core::lock(&ctx.drawing.job);
        let queued = job.status.active_key == request.key || job.pending.contains(&request.key);
        if job.enable.remove(&request.key) && queued {
            if job.library {
                job.pending.retain(|pending| *pending != request.key);
                job.count();
                job.publish(ctx);
            } else {
                drop(job);
                ctx.drawing.cancel(ctx, Phase::Cancelled);
            }
        }
        return preference::save(ctx, &request.key, false);
    }
    if cached(ctx, &request.key, &source).is_ok() {
        preference::save(ctx, &request.key, true)?;
        let mut job = skwd_wall_core::lock(&ctx.drawing.job);
        if job.status.active_key == request.key
            && job.thread.as_ref().is_none_or(std::thread::JoinHandle::is_finished)
        {
            job.status.phase = Phase::Idle;
            job.status.error = None;
            job.status.stage.clear();
        }
        return Ok(());
    }
    ctx.drawing.prepare(
        ctx,
        vec![request.key.clone()],
        BTreeSet::from([request.key]),
        BTreeSet::new(),
    )
}

fn source(ctx: &Ctx, key: &str) -> Result<PathBuf> {
    let config = ctx.state.config();
    let path = if let Some(name) = key.strip_prefix(wall_proto::drawing::STATIC_KEY_PREFIX) {
        Path::new(&config.wallpaper_dir()).join(name).canonicalize()?
    } else if let Some(name) = key.strip_prefix(wall_proto::drawing::VIDEO_KEY_PREFIX) {
        Path::new(&config.video_dir()).join(name).canonicalize()?
    } else if let Some(we_id) = key.strip_prefix(wall_proto::drawing::WE_KEY_PREFIX) {
        ensure!(skwd_wall_core::we::valid_we_id(we_id), INVALID_WE_ID);
        let path = config.we_dir().join(we_id).canonicalize()?;
        let project = paper_control::we_project::Project::resolve(&path)?;
        let kind = project.kind().unwrap_or_default();
        ensure!(skwd_wall_core::we::is_supported_project(&kind), UNSUPPORTED_WE_PROJECT);
        if kind == skwd_wall_core::we::SCENE_PROJECT_TYPE {
            project.scene_package()?;
        } else {
            project.video_file()?;
        }
        path
    } else {
        anyhow::bail!("choose a library wallpaper");
    };
    if !key.starts_with(wall_proto::drawing::WE_KEY_PREFIX) {
        ensure!(
            ctx.state.library_key(&path).as_deref() == Some(key),
            "wallpaper is outside the library"
        );
    }
    ensure!(
        ctx.state.with_db(|connection| Ok(skwd_wall_core::db::has_entry(connection, key)))?,
        "wallpaper is not in the library"
    );
    Ok(path)
}

fn cached(ctx: &Ctx, key: &str, source: &Path) -> Result<PathBuf> {
    let config = ctx.state.config();
    if key.starts_with(wall_proto::drawing::STATIC_KEY_PREFIX) {
        cache::lookup(&config, source)
    } else {
        let signature = cache::media_signature(&ctx.state, key, source)?;
        cache::lookup_media(&config, source, &signature).map(|(manifest, _)| manifest)
    }
}

pub(crate) fn tidy(ctx: &Ctx) {
    let _control = skwd_wall_core::lock(&ctx.drawing.control);
    {
        let mut job = skwd_wall_core::lock(&ctx.drawing.job);
        if job.activity != Activity::Stopped || !job.idle() {
            job.plan |= job.library;
            return;
        }
    }
    if let Err(error) = cache::prune(&ctx.state.config()) {
        log::warn!("drawing: cache prune failed: {error:#}");
    }
}

pub(crate) fn control(ctx: &Ctx, action: wall_proto::TaskControl) -> bool {
    let _control = skwd_wall_core::lock(&ctx.drawing.control);
    let config = ctx.state.config().clone();
    let library = skwd_wall_core::lock(&ctx.drawing.job).library;
    if !library && !batch::held(&config) {
        return false;
    }
    let result = match action {
        wall_proto::TaskControl::Pause if library => pause(ctx),
        wall_proto::TaskControl::Pause => Ok(()),
        wall_proto::TaskControl::Resume => ctx.drawing.prepare_library(ctx, true),
        wall_proto::TaskControl::Stop if library => stop(ctx),
        wall_proto::TaskControl::Stop => {
            ctx.tasks.finish_if_active(
                wall_proto::drawing::TASK,
                wall_proto::TaskState::Cancelled,
                "",
            );
            batch::stop(ctx)
        }
    };
    if let Err(error) = &result {
        log::warn!("drawing: task control failed: {error:#}");
    }
    let status = ctx.drawing.status(ctx, "");
    publish(ctx, &status);
    result.is_ok()
}

fn pause(ctx: &Ctx) -> Result<()> {
    let library = skwd_wall_core::lock(&ctx.drawing.job).library;
    ctx.drawing.cancel(ctx, Phase::Paused);
    if library {
        batch::hold(&ctx.state.config(), true)?;
    }
    Ok(())
}

fn resume(ctx: &Ctx) -> Result<()> {
    let job = skwd_wall_core::lock(&ctx.drawing.job);
    if job.library {
        drop(job);
        return ctx.drawing.prepare_library(ctx, true);
    }
    let keys = Vec::from(job.pending.clone());
    let enable = job.enable.clone();
    let outlines = job.outlines.clone();
    drop(job);
    ctx.drawing.prepare(ctx, keys, enable, outlines)
}

fn stop(ctx: &Ctx) -> Result<()> {
    let library = skwd_wall_core::lock(&ctx.drawing.job).library;
    ctx.drawing.cancel(ctx, Phase::Cancelled);
    if library {
        batch::stop(ctx)?;
    }
    Ok(())
}

fn clear(ctx: &Ctx) -> Result<()> {
    ctx.drawing.cancel(ctx, Phase::Idle);
    cache::clear(&ctx.state.config())
}

fn preview(ctx: &Ctx, request: &KeyRequest) -> Result<()> {
    let source = source(ctx, &request.key)?;
    skwd_wall_core::infrastructure::drawing::preview(
        &ctx.state,
        &request.key,
        &source,
        request.output.as_deref().unwrap_or(wall_proto::apply_fields::ALL_OUTPUTS),
    )
}

fn can_prepare(config: &skwd_wall_core::config::Config) -> bool {
    skwd_config::bool_at(config.root(), skwd_config::keys::drawing::ON_BATTERY, false)
        || !skwd_config::on_battery_power()
}

fn publish(ctx: &Ctx, status: &Status) {
    ctx.events.publish(ev::DRAWING_STATUS, serde_json::json!(status));
}

fn update_config(change: impl FnOnce(&mut serde_json::Value)) -> Result<()> {
    static CONFIG_WRITE: Mutex<()> = Mutex::new(());
    let _guard = skwd_wall_core::lock(&CONFIG_WRITE);
    let path = skwd_config::config_path();
    let mut config = skwd_wall_core::config::Config::load().root().clone();
    if !config.is_object() {
        config = serde_json::json!({});
    }
    let root = skwd_config::keys::drawing::ROOT;
    if !config[root].is_object() {
        config[root] = serde_json::json!({});
    }
    change(&mut config[root]);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    skwd_config::atomic_write(&path, &serde_json::to_vec_pretty(&config)?)?;
    Ok(())
}

#[cfg(test)]
mod tests;
