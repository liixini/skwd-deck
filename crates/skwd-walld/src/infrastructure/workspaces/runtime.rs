use skwd_wall_core::lock;
use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::Value;
use skwd_wall_core::WallState;
use skwd_wall_core::backend::wallpaper::WallpaperApplication;

use crate::backend::events::EventPublisher;
use crate::backend::history::{ApplySource, HistoryRepository};
use crate::composition::apply::apply_by_key;
use crate::infrastructure::stats::Stats;

#[allow(clippy::wildcard_imports)]
use super::model::*;
#[cfg(test)]
use super::niri::{parse_activated, parse_topology};
#[allow(clippy::wildcard_imports)]
use super::policy::*;
#[allow(clippy::wildcard_imports)]
use super::provider::*;
#[allow(clippy::wildcard_imports)]
use super::storage::*;
use super::{hypr, kwin, niri};

pub(super) const RECONNECT_DELAY: Duration = Duration::from_secs(2);

pub(super) struct Engine {
    pub(super) rt: Mutex<WorkspaceRuntime>,
    pub(super) cv: Condvar,
}

static ENGINE: OnceLock<Arc<Engine>> = OnceLock::new();

pub(super) fn rt_lock(engine: &Engine) -> std::sync::MutexGuard<'_, WorkspaceRuntime> {
    lock(&engine.rt)
}

pub fn list() -> Vec<wall_proto::WorkspaceRow> {
    let Some(engine) = ENGINE.get() else {
        return Vec::new();
    };
    let mut rows: Vec<wall_proto::WorkspaceRow> = {
        let rt = rt_lock(engine);
        rt.topo
            .values()
            .map(|ws| wall_proto::WorkspaceRow {
                output: ws.output.clone(),
                idx: ws.idx,
                name: ws.name.clone(),
                active: ws.active,
            })
            .collect()
    };
    rows.sort_by(|left, right| left.output.cmp(&right.output).then(left.idx.cmp(&right.idx)));
    rows
}

pub fn reload(state: &WallState) {
    let Some(engine) = ENGINE.get() else {
        return;
    };
    state.reload_config();
    let (enabled, rules, debounce) = {
        let cfg = state.config();
        (
            cfg.workspace_enabled(),
            parse_rules(&cfg.workspace_wallpapers()),
            cfg.workspace_debounce_ms(),
        )
    };
    if !enabled {
        return;
    }
    let deadline = Instant::now() + Duration::from_millis(debounce);
    let mut rt = rt_lock(engine);
    if refresh_pending(&mut rt, &rules, deadline) {
        drop(rt);
        engine.cv.notify_all();
    }
}

fn take_due(engine: &Engine) -> Vec<(String, DesiredWallpaper, Option<&'static str>)> {
    let mut rt = rt_lock(engine);
    while rt.pending.is_empty() {
        rt = engine.cv.wait(rt).unwrap_or_else(std::sync::PoisonError::into_inner);
    }
    loop {
        let now = Instant::now();
        match rt.deadline {
            Some(deadline) if deadline > now => {
                let (guard, _) = engine
                    .cv
                    .wait_timeout(rt, deadline - now)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                rt = guard;
            }
            _ => break,
        }
    }
    rt.deadline = None;
    let taken: Vec<(String, DesiredWallpaper)> = rt.pending.drain().collect();
    let mut out = Vec::new();
    for (name, want) in taken {
        if let DesiredWallpaper::Pin(key) = &want
            && rt.last.get(&name) == Some(key)
        {
            continue;
        }
        let dir = rt.dirs.remove(&name);
        out.push((name, want, dir));
    }
    out
}

pub fn note_external_apply(
    state: &WallState,
    output: &str,
    ty: &str,
    path: &str,
    we_id: &str,
    mute: bool,
    volume: u32,
) {
    let Some(engine) = ENGINE.get() else {
        return;
    };
    let outputs: Vec<String> =
        if output == "*" { skwd_wall_core::outputs::names() } else { vec![output.to_string()] };
    if outputs.is_empty() {
        return;
    }
    let entry = BaseWallpaper {
        ty: ty.to_string(),
        path: path.to_string(),
        we_id: we_id.to_string(),
        mute,
        volume,
    };
    let cache = state.config().cache_dir();
    let json = {
        let mut rt = rt_lock(engine);
        for out in &outputs {
            rt.base.insert(out.clone(), entry.clone());
            rt.last.remove(out);
        }
        base_to_json(&rt.base)
    };
    let _ = skwd_wall_core::paths::atomic_write(&base_file(&cache), json.to_string().as_bytes());
}

fn worker_loop(
    engine: &Arc<Engine>,
    state: &Arc<WallState>,
    application: &dyn WallpaperApplication,
    history: &dyn HistoryRepository,
    publisher: &dyn EventPublisher,
    stats: &Arc<Stats>,
) {
    loop {
        for (output, desired, dir) in take_due(engine) {
            log::info!("workspace: applying {desired:?} to {output} (dir={dir:?})");
            stats.set_task("workspace");
            let slide_ms = state.config().workspace_slide_ms();
            if let Some(dir) = dir.filter(|_| slide_ms > 0) {
                state.apply().set_swap_slide(Some((dir.to_string(), slide_ms)));
            }
            match desired {
                DesiredWallpaper::Pin(key) => {
                    let ok = apply_by_key(
                        state,
                        application,
                        history,
                        publisher,
                        stats,
                        &key,
                        &output,
                        false,
                        ApplySource::Workspace,
                    );
                    if ok {
                        rt_lock(engine).last.insert(output.clone(), key);
                        push_preload(engine, state, &output);
                    }
                }
                DesiredWallpaper::Base => {
                    let entry = rt_lock(engine).base.get(&output).cloned();
                    if let Some(entry) = entry {
                        if apply_base(
                            state,
                            application,
                            history,
                            publisher,
                            stats,
                            &entry,
                            &output,
                        ) {
                            rt_lock(engine).last.remove(&output);
                            push_preload(engine, state, &output);
                        }
                    } else {
                        state.apply().set_swap_slide(None);
                    }
                }
            }
            stats.set_task("idle");
        }
    }
}

fn apply_base(
    state: &Arc<WallState>,
    application: &dyn WallpaperApplication,
    history: &dyn HistoryRepository,
    publisher: &dyn EventPublisher,
    stats: &Stats,
    entry: &BaseWallpaper,
    output: &str,
) -> bool {
    match crate::composition::apply::apply_core(
        state,
        application,
        history,
        publisher,
        stats,
        &entry.ty,
        &entry.path,
        &entry.we_id,
        entry.mute,
        entry.volume,
        ApplySource::Workspace,
        output,
        false,
        false,
        None,
        None,
    ) {
        Ok(_) => true,
        Err(err) => {
            log::warn!("workspace: base restore failed for {output}: {err}");
            false
        }
    }
}

fn push_preload(engine: &Engine, state: &WallState, output: &str) {
    let (rules, wdir, vdir) = {
        let cfg = state.config();
        (parse_rules(&cfg.workspace_wallpapers()), cfg.wallpaper_dir(), cfg.video_dir())
    };
    let base = rt_lock(engine).base.get(output).cloned();
    let paths = preload_paths(&rules, base.as_ref(), output, &wdir, &vdir);
    let fill = state.config().display().fill_mode_for(output);
    if !paths.is_empty() && state.renderers().output_still_preload(output, paths.clone(), &fill) {
        log::info!("workspace: preload pushed to {output} ({} images)", paths.len());
    }
}

pub(super) fn ingest_snapshot(
    engine: &Engine,
    state: &WallState,
    topo: HashMap<u64, WorkspaceInfo>,
) {
    state.reload_config();
    let (enabled, rules, debounce) = {
        let cfg = state.config();
        (
            cfg.workspace_enabled(),
            parse_rules(&cfg.workspace_wallpapers()),
            cfg.workspace_debounce_ms(),
        )
    };
    let mut rt = rt_lock(engine);
    let old_active: HashMap<String, u64> =
        rt.topo.values().filter(|ws| ws.active).map(|ws| (ws.output.clone(), ws.idx)).collect();
    for ws in topo.values().filter(|ws| ws.active) {
        if let Some(&old) = old_active.get(&ws.output)
            && old != ws.idx
        {
            rt.dirs.insert(ws.output.clone(), if ws.idx > old { "up" } else { "down" });
        }
    }
    rt.topo = topo;
    if !enabled {
        return;
    }
    let deadline = Instant::now() + Duration::from_millis(debounce);
    if refresh_pending(&mut rt, &rules, deadline) {
        log::info!("workspace: pending now {:?}", rt.pending);
        drop(rt);
        engine.cv.notify_all();
    }
}

pub fn start(ctx: crate::composition::context::Ctx) {
    let crate::composition::context::Ctx { state, wallpaper, history, events, stats, .. } = ctx;
    if !state.config().workspace_enabled() {
        return;
    }
    let Some(backend) = detect_backend() else {
        return;
    };
    log::info!("workspace: backend {backend:?}");
    let (base, last) = {
        let cfg = state.config();
        let cache = cfg.cache_dir();
        let rules = parse_rules(&cfg.workspace_wallpapers());
        let raw = std::fs::read_to_string(base_file(&cache)).unwrap_or_default();
        let base = base_from_json(&serde_json::from_str(&raw).unwrap_or(Value::Null));
        let last = seed_last(
            &skwd_wall_core::audio::read_state(&cache),
            &rules,
            &cfg.wallpaper_dir(),
            &cfg.video_dir(),
        );
        (base, last)
    };
    if !base.is_empty() || !last.is_empty() {
        log::info!(
            "workspace: restored {} base entrie(s), seeded {} pinned output(s)",
            base.len(),
            last.len()
        );
    }
    let engine = Arc::new(Engine {
        rt: Mutex::new(WorkspaceRuntime {
            topo: HashMap::new(),
            pending: HashMap::new(),
            last,
            base,
            dirs: HashMap::new(),
            deadline: None,
        }),
        cv: Condvar::new(),
    });
    let _ = ENGINE.set(Arc::clone(&engine));
    let reader_engine = Arc::clone(&engine);
    let reader_state = Arc::clone(&state);
    std::thread::spawn(move || match backend {
        Backend::Niri => niri::reader_loop(&reader_engine, &reader_state),
        Backend::Hyprland => hypr::reader_loop(&reader_engine, &reader_state),
        Backend::Kwin => kwin::reader_loop(&reader_engine, &reader_state),
    });
    std::thread::spawn(move || {
        worker_loop(&engine, &state, wallpaper.as_ref(), history.as_ref(), events.as_ref(), &stats);
    });
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
