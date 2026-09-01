use crate::lock;
use std::collections::HashMap;

use wayland_client::protocol::{wl_output, wl_registry};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};

#[derive(Clone, Default)]
pub struct OutputInfo {
    pub name: String,
    pub make: String,
    pub model: String,
    pub width: i32,
    pub height: i32,
    pub refresh_mhz: i32,
    pub scale: i32,
    pub rotated: bool,
}

impl OutputInfo {
    pub fn logical_size(&self) -> (i32, i32) {
        if self.rotated { (self.height, self.width) } else { (self.width, self.height) }
    }

    pub fn identity(&self) -> String {
        monitor_identity(&self.make, &self.model, &self.name)
    }

    pub fn identity_key(&self) -> String {
        let identity = self.identity();
        if identity == self.name { identity } else { format!("{identity} @ {}", self.name) }
    }

    pub fn portrait(&self) -> bool {
        let (width, height) = self.logical_size();
        height > width
    }
}

pub fn monitor_identity(make: &str, model: &str, name: &str) -> String {
    let make = make.trim();
    let model = model.trim();
    if make.is_empty() && model.is_empty() {
        return name.to_string();
    }
    format!("{make} {model}").trim().to_string()
}

pub fn effective_fps(limit: u32, refresh_mhz: i32) -> u32 {
    if refresh_mhz <= 0 {
        return limit.max(1);
    }
    let refresh = ((refresh_mhz as u32).saturating_add(500) / 1000).max(1);
    limit.max(1).min(refresh)
}

pub fn target_fps(limit: u32, target: &str, outputs: &[OutputInfo]) -> u32 {
    let selected = |output: &OutputInfo| {
        target == "*" || target.split(',').any(|name| name.trim() == output.name)
    };
    outputs
        .iter()
        .filter(|output| selected(output))
        .map(|output| effective_fps(limit, output.refresh_mhz))
        .max()
        .unwrap_or_else(|| limit.max(1))
}

pub fn refresh_signature(outputs: &[OutputInfo]) -> String {
    let mut values = outputs
        .iter()
        .map(|output| format!("{}:{}", output.name, output.refresh_mhz))
        .collect::<Vec<_>>();
    values.sort();
    values.join(",")
}

pub fn fps_map(limit: u32, outputs: &[OutputInfo]) -> String {
    let mut values = outputs
        .iter()
        .map(|output| format!("{}={}", output.name, effective_fps(limit, output.refresh_mhz)))
        .collect::<Vec<_>>();
    values.sort();
    values.join(";")
}

#[derive(Default)]
struct Acc {
    name: String,
    make: String,
    model: String,
    width: i32,
    height: i32,
    refresh_mhz: i32,
    scale: i32,
    rotated: bool,
}

#[derive(Default)]
struct Collector {
    next: u32,
    outs: HashMap<u32, Acc>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for Collector {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        (): &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { name, interface, version } = event
            && interface == "wl_output"
        {
            let id = state.next;
            state.next += 1;
            state.outs.insert(id, Acc::default());
            let _: wl_output::WlOutput = registry.bind(name, version.min(4), qh, id);
        }
    }
}

impl Dispatch<wl_output::WlOutput, u32> for Collector {
    fn event(
        state: &mut Self,
        _: &wl_output::WlOutput,
        event: wl_output::Event,
        id: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let acc = state.outs.entry(*id).or_default();
        match event {
            wl_output::Event::Name { name } => acc.name = name,
            wl_output::Event::Mode { flags, width, height, refresh } => {
                if matches!(flags, WEnum::Value(mflags) if mflags.contains(wl_output::Mode::Current))
                {
                    acc.width = width;
                    acc.height = height;
                    acc.refresh_mhz = refresh;
                }
            }
            wl_output::Event::Scale { factor } => acc.scale = factor,
            wl_output::Event::Geometry { transform, make, model, .. } => {
                acc.make = make;
                acc.model = model;
                acc.rotated = matches!(
                    transform,
                    WEnum::Value(
                        wl_output::Transform::_90
                            | wl_output::Transform::_270
                            | wl_output::Transform::Flipped90
                            | wl_output::Transform::Flipped270
                    )
                );
            }
            _ => {}
        }
    }
}

const ENUM_TTL: std::time::Duration = std::time::Duration::from_secs(2);
const MAX_ROUNDTRIPS: usize = 10;
const ENUM_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(400);

fn enum_cache() -> &'static std::sync::Mutex<Option<(Vec<OutputInfo>, std::time::Instant)>> {
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<Option<(Vec<OutputInfo>, std::time::Instant)>>,
    > = std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(None))
}

pub fn invalidate() {
    *lock(enum_cache()) = None;
}

#[cfg(test)]
fn enum_test_lock() -> &'static std::sync::RwLock<()> {
    static LOCK: std::sync::OnceLock<std::sync::RwLock<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::RwLock::new(()))
}

#[cfg(test)]
pub(crate) fn enum_exclusive() -> std::sync::RwLockWriteGuard<'static, ()> {
    enum_test_lock().write().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(all(test, feature = "daemon"))]
pub(crate) fn enum_shared() -> std::sync::RwLockReadGuard<'static, ()> {
    enum_test_lock().read().unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub fn enumerate() -> Vec<OutputInfo> {
    if let Some(fake) = fake_outputs() {
        return fake;
    }
    enumerate_with(enumerate_live)
}

fn fake_spec() -> Option<String> {
    if let Some(path) = std::env::var_os("SKWD_FAKE_OUTPUTS_FILE") {
        return Some(std::fs::read_to_string(path).unwrap_or_default().trim().to_string());
    }
    std::env::var("SKWD_FAKE_OUTPUTS").ok()
}

fn fake_active() -> bool {
    std::env::var_os("SKWD_FAKE_OUTPUTS_FILE").is_some()
        || std::env::var_os("SKWD_FAKE_OUTPUTS").is_some()
}

fn fake_outputs() -> Option<Vec<OutputInfo>> {
    let spec = fake_spec().filter(|val| !val.is_empty())?;
    Some(spec.split(',').filter_map(parse_fake_output).collect())
}

fn parse_fake_output(entry: &str) -> Option<OutputInfo> {
    let (name, dims) = entry.split_once(':').unwrap_or((entry, "1920x1080"));
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let (width, height) = dims.split_once('x').unwrap_or(("1920", "1080"));
    Some(OutputInfo {
        name: name.to_string(),
        make: String::new(),
        model: String::new(),
        width: width.trim().parse().unwrap_or(1920),
        height: height.trim().parse().unwrap_or(1080),
        refresh_mhz: 0,
        scale: 1,
        rotated: false,
    })
}

fn enumerate_with(fetch: impl FnOnce() -> Vec<OutputInfo>) -> Vec<OutputInfo> {
    if let Some((outs, at)) = lock(enum_cache()).as_ref()
        && at.elapsed() < ENUM_TTL
    {
        return outs.clone();
    }
    let fresh = fetch();
    if !fresh.is_empty() {
        *lock(enum_cache()) = Some((fresh.clone(), std::time::Instant::now()));
    }
    fresh
}

fn enumerate_live() -> Vec<OutputInfo> {
    let Ok(conn) = Connection::connect_to_env() else {
        return Vec::new();
    };
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    let _registry = conn.display().get_registry(&qh, ());
    let mut state = Collector::default();
    if queue.roundtrip(&mut state).is_err() {
        return Vec::new();
    }
    for _ in 0..MAX_ROUNDTRIPS {
        if queue.roundtrip(&mut state).is_err() {
            break;
        }
        if !state.outs.is_empty() && state.outs.values().all(|acc| !acc.name.is_empty()) {
            break;
        }
    }
    let mut outs: Vec<OutputInfo> = state
        .outs
        .into_values()
        .filter(|acc| !acc.name.is_empty())
        .map(|acc| OutputInfo {
            name: acc.name,
            make: acc.make,
            model: acc.model,
            width: acc.width,
            height: acc.height,
            refresh_mhz: acc.refresh_mhz,
            scale: acc.scale.max(1),
            rotated: acc.rotated,
        })
        .collect();
    outs.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));
    outs.dedup_by(|lhs, rhs| lhs.name == rhs.name);
    outs
}

pub fn names() -> Vec<String> {
    enumerate().into_iter().map(|out| out.name).collect()
}

#[derive(Default)]
struct Watcher {
    outputs: std::collections::HashSet<u32>,
    changed: bool,
}

impl Dispatch<wl_registry::WlRegistry, ()> for Watcher {
    fn event(
        state: &mut Self,
        _: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global { name, interface, .. } if interface == "wl_output" => {
                if state.outputs.insert(name) {
                    state.changed = true;
                }
            }
            wl_registry::Event::GlobalRemove { name } if state.outputs.remove(&name) => {
                state.changed = true;
            }
            _ => {}
        }
    }
}

pub fn watch<F: FnMut()>(mut on_change: F) {
    if fake_active() {
        return watch_fake(on_change);
    }
    let Ok(conn) = Connection::connect_to_env() else {
        return;
    };
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    let _registry = conn.display().get_registry(&qh, ());
    let mut watcher = Watcher::default();
    if queue.roundtrip(&mut watcher).is_err() {
        return;
    }
    watcher.changed = false;
    loop {
        if queue.blocking_dispatch(&mut watcher).is_err() {
            break;
        }
        if !watcher.changed {
            continue;
        }
        watcher.changed = false;
        std::thread::sleep(ENUM_RETRY_DELAY);
        let _ = queue.roundtrip(&mut watcher);
        watcher.changed = false;
        on_change();
    }
}

fn watch_fake<F: FnMut()>(mut on_change: F) {
    let names =
        || fake_outputs().map(|outs| outs.into_iter().map(|out| out.name).collect::<Vec<_>>());
    let mut last = names();
    loop {
        std::thread::sleep(std::time::Duration::from_millis(150));
        let now = names();
        if now != last {
            last = now;
            invalidate();
            on_change();
        }
    }
}

#[path = "tests.rs"]
mod tests;
