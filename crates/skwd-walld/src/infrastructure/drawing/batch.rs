use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use anyhow::Result;
use skwd_config::keys::drawing as keys;
use skwd_wall_core::infrastructure::drawing as cache;
use wall_proto::drawing::{Phase, STATIC_KEY_PREFIX, Status, TASK};
use wall_proto::{TaskCapabilities, TaskState, TaskStatus};

use crate::composition::context::Ctx;

const TASK_LABEL: &str = "Preparing drawings";

pub(super) struct Plan {
    pub(super) keys: Vec<String>,
    pub(super) done: u32,
}

pub(super) fn plan(ctx: &Ctx, pack: &str) -> Result<Plan> {
    let config = ctx.state.config().clone();
    if let Err(error) = cache::prune(&config) {
        log::warn!("drawing: cache prune failed: {error:#}");
    }
    let keys = ctx.state.with_db(skwd_wall_core::db::drawable_keys)?;
    let behind = crate::infrastructure::widgets::behind(config.root());
    let mut plan = Plan { keys: Vec::new(), done: 0 };
    for key in keys {
        let outlined = behind && key.starts_with(STATIC_KEY_PREFIX);
        if !outlined && !skwd_config::drawing_enabled(config.root(), &key) {
            continue;
        }
        let Ok(source) = super::source(ctx, &key) else {
            continue;
        };
        let recorded = if key.starts_with(STATIC_KEY_PREFIX) {
            cache::recorded(&config, &source, None, pack)
        } else {
            cache::media_signature(&ctx.state, &key, &source)
                .is_ok_and(|signature| cache::recorded(&config, &source, Some(&signature), pack))
        };
        if recorded {
            plan.done += 1;
        } else {
            plan.keys.push(key);
        }
    }
    Ok(plan)
}

pub(super) fn merge(ctx: &Ctx, plan: Plan) {
    let queued = Vec::from(skwd_wall_core::lock(&ctx.drawing.job).pending.clone());
    let gone: HashSet<String> =
        queued.into_iter().filter(|key| super::source(ctx, key).is_err()).collect();
    let mut job = skwd_wall_core::lock(&ctx.drawing.job);
    let job = &mut *job;
    let planned: HashSet<&String> = plan.keys.iter().collect();
    job.failed.retain(|key| planned.contains(key));
    job.pending.retain(|key| !gone.contains(key));
    job.enable.retain(|key| !gone.contains(key));
    let requested: Vec<String> = job.enable.iter().cloned().collect();
    let mut queued: HashSet<String> = job.pending.iter().cloned().collect();
    for key in requested.into_iter().chain(plan.keys) {
        if !job.failed.contains(&key) && queued.insert(key.clone()) {
            job.pending.push_back(key);
        }
    }
    job.status.completed = plan.done;
    job.status.failed = job.failed.len() as u32;
    job.count();
    job.publish(ctx);
}

pub(super) fn report(ctx: &Ctx, status: &Status) {
    let state = match status.phase {
        Phase::Preparing | Phase::Checking => TaskState::Running,
        Phase::Paused => TaskState::Paused,
        Phase::Ready => {
            ctx.tasks.finish(TASK, TaskState::Completed, String::new());
            return;
        }
        Phase::Failed => {
            ctx.tasks.finish(TASK, TaskState::Failed, status.error.clone().unwrap_or_default());
            return;
        }
        Phase::Idle | Phase::Cancelled => {
            ctx.tasks.finish_if_active(TASK, TaskState::Cancelled, String::new());
            return;
        }
    };
    let running = state == TaskState::Running;
    let mut task = TaskStatus::running(TASK, TASK, TASK_LABEL);
    task.state = state;
    task.progress = u64::from(status.completed + status.failed);
    task.total = u64::from(status.total);
    task.capabilities = TaskCapabilities { pause: running, resume: !running, stop: true };
    ctx.tasks.update(task);
}

pub(crate) fn auto(ctx: &Ctx) {
    let _control = skwd_wall_core::lock(&ctx.drawing.control);
    let config = ctx.state.config().clone();
    if held(&config) {
        if !ctx.tasks.list().iter().any(|task| task.id == TASK) {
            report(ctx, &Status { phase: Phase::Paused, ..Status::default() });
        }
        return;
    }
    if !skwd_config::bool_at(config.root(), keys::AUTO_PREPARE, false)
        || !super::can_prepare(&config)
        || super::pack::Pack::load(&config).is_err()
    {
        return;
    }
    let paused = {
        let job = skwd_wall_core::lock(&ctx.drawing.job);
        job.status.phase == Phase::Paused && !job.library && !job.pending.is_empty()
    };
    if paused {
        return;
    }
    if let Err(error) = ctx.drawing.prepare_library(ctx, false) {
        log::warn!("drawing: automatic preparation did not start: {error:#}");
    }
}

pub(crate) fn outline(ctx: &Ctx, source: &Path) {
    let Some(key) = ctx.state.library_key(source) else { return };
    if !key.starts_with(STATIC_KEY_PREFIX) {
        return;
    }
    let _control = skwd_wall_core::lock(&ctx.drawing.control);
    if let Err(error) = queue_outline(ctx, source, key.clone()) {
        log::warn!("drawing: no outline for {key}: {error:#}");
    }
}

fn queue_outline(ctx: &Ctx, source: &Path, key: String) -> Result<()> {
    let config = ctx.state.config().clone();
    let pack = super::pack::Pack::load(&config)?;
    if held(&config) || cache::recorded(&config, source, None, &pack.fingerprint) {
        return Ok(());
    }
    let mut job = skwd_wall_core::lock(&ctx.drawing.job);
    let stopped = job.activity == super::Activity::Stopped;
    if job.status.phase == Phase::Paused || (stopped && (job.library || job.pending.len() > 1)) {
        return Ok(());
    }
    if (!job.library && job.pending.contains(&key)) || (!stopped && job.status.active_key == key) {
        return Ok(());
    }
    if stopped {
        drop(job);
        let outlines = BTreeSet::from([key.clone()]);
        return ctx.drawing.prepare(ctx, vec![key], BTreeSet::new(), outlines);
    }
    job.outlines.insert(key.clone());
    if job.library {
        job.jump(&[key]);
    } else {
        job.pending.push_back(key);
    }
    job.count();
    job.publish(ctx);
    Ok(())
}

pub(super) fn held(config: &skwd_wall_core::config::Config) -> bool {
    cache::root(config).join(cache::PAUSE_MARKER).exists()
}

pub(super) fn hold(config: &skwd_wall_core::config::Config, held: bool) -> Result<()> {
    let marker = cache::root(config).join(cache::PAUSE_MARKER);
    if held {
        std::fs::create_dir_all(cache::root(config))?;
        std::fs::write(marker, [])?;
    } else if marker.exists() {
        std::fs::remove_file(marker)?;
    }
    Ok(())
}

pub(super) fn stop(ctx: &Ctx) -> Result<()> {
    hold(&ctx.state.config(), false)?;
    super::update_config(|drawing| {
        drawing[keys::AUTO_PREPARE_FIELD] = serde_json::Value::Bool(false);
    })?;
    super::preference::changed(ctx);
    Ok(())
}
