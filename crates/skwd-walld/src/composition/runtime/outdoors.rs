use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value;
use skwd_config::keys::weather;
use skwd_wall_core::{WallState, db, lock};
use wall_proto::{
    OutdoorMatch, OutdoorOutput, OutdoorStatus, WallpaperItem, WeatherSource, WeatherState,
    WeatherStatus, ev,
};
use wall_rules::outdoors::{Conditions, Context};

use crate::backend::events::EventPublisher;
use crate::backend::history::ApplySource;
use crate::composition::context::Ctx;
use crate::infrastructure::weather::Forecast;

mod session;
use session::{Session, save_config};

const FOLLOW_DISABLED: &str = "Follow outdoors is disabled";
const STALE_WEATHER: &str = "weather is unavailable; current wallpaper is retained";
const UNKNOWN_OUTPUT: &str = "unknown wallpaper output";
const SESSION_CONTEXTS: &str = "contexts";
const SESSION_FILE: &str = "outdoors-session.json";
const ALL_OUTPUTS: &str = "*";
const ITEM_TYPE: &str = "type";
const ITEM_PATH: &str = "path";
const ITEM_WE_ID: &str = "we_id";
const SECONDS_PER_MINUTE: i64 = 60;
const MINUTES_PER_DAY: i64 = 1440;
const HUE_CIRCLE: i64 = 360;
const PALETTE_DISTANCE: i64 = 45;
const MAX_ASPECT_CHANGE: f64 = 2.0;

struct Runtime {
    path: PathBuf,
    session: Session,
    config: skwd_wall_core::config::Config,
    status: WeatherStatus,
    context: Option<Context>,
    pending: Option<(Context, Instant)>,
    forecast: Forecast,
    generation: u64,
    restore_config: Option<skwd_wall_core::config::Config>,
    plans: BTreeMap<String, Vec<String>>,
    due: BTreeSet<String>,
    legacy_outputs: BTreeSet<String>,
}

impl Runtime {
    fn needs_change(
        &mut self,
        output: &str,
        current: Option<&String>,
        chosen: &str,
        context: &Context,
    ) -> bool {
        if self.legacy_outputs.remove(output)
            && !self.session.contexts.contains_key(output)
            && self.session.owned.get(output).or_else(|| self.session.wallpapers.get(output))
                == current
            && current.is_some()
        {
            self.session.contexts.insert(output.to_owned(), context.clone());
        }
        let applied = self.session.contexts.get(output) == Some(context);
        if current.map(String::as_str) != Some(chosen) {
            return !applied || current.is_none();
        }
        if !applied {
            self.session.contexts.insert(output.to_owned(), context.clone());
        }
        false
    }
}

static RUNTIME: Mutex<Option<Runtime>> = Mutex::new(None);
static CONTROL: Mutex<()> = Mutex::new(());
static GENERATION: AtomicU64 = AtomicU64::new(1);

fn generation() -> u64 {
    GENERATION.fetch_add(1, Ordering::Relaxed)
}

fn load(state: &Arc<WallState>) -> Runtime {
    let config = state.config().clone();
    let path = PathBuf::from(config.cache_dir()).join(SESSION_FILE);
    let saved =
        std::fs::read(&path).ok().and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    let legacy = saved.as_ref().is_some_and(|saved| saved.get(SESSION_CONTEXTS).is_none());
    let mut session: Session =
        saved.and_then(|saved| serde_json::from_value(saved).ok()).unwrap_or_default();
    let legacy_outputs = if legacy && session.active {
        session.owned.keys().chain(session.wallpapers.keys()).cloned().collect()
    } else {
        BTreeSet::new()
    };
    if let Err(error) = session.recover(state, &path) {
        log::warn!("outdoors: session recovery failed: {error}");
    }
    let current_location = location_key(&config);
    let context = if session.location_key.as_deref() == Some(current_location.as_str()) {
        session.contexts.values().next().cloned()
    } else {
        session.contexts.clear();
        None
    };
    Runtime {
        path,
        session,
        status: make_status(&config, &Forecast::default()),
        config,
        context,
        pending: None,
        forecast: Forecast::default(),
        generation: generation(),
        restore_config: None,
        plans: BTreeMap::new(),
        due: BTreeSet::new(),
        legacy_outputs,
    }
}

fn initialized<'a>(guard: &'a mut Option<Runtime>, state: &Arc<WallState>) -> &'a mut Runtime {
    let path = PathBuf::from(state.config().cache_dir()).join(SESSION_FILE);
    if guard.as_ref().is_none_or(|runtime| runtime.path != path) {
        *guard = Some(load(state));
    }
    guard.as_mut().expect("outdoors runtime initialized")
}

pub(crate) fn active(config: &skwd_wall_core::config::Config) -> bool {
    config.weather_source() == WeatherSource::Outdoors && config.weather_enabled()
}

pub(crate) fn current_keys(state: &Arc<WallState>) -> BTreeMap<String, String> {
    let config = state.config().clone();
    let audio = skwd_wall_core::audio::read_state(&config.cache_dir());
    let mut keys = BTreeMap::new();
    for (output, entry) in audio.as_object().into_iter().flatten() {
        let Some(kind) = entry.get(ITEM_TYPE).and_then(Value::as_str) else { continue };
        let field = if kind == wall_proto::kind::WE { ITEM_WE_ID } else { ITEM_PATH };
        if let Some(path) = entry.get(field).and_then(Value::as_str) {
            let key = if kind == wall_proto::kind::WE {
                None
            } else {
                state.library_key(std::path::Path::new(path))
            };
            keys.insert(output.clone(), key.unwrap_or_else(|| format!("{kind}:{path}")));
        }
    }
    if let Some(key) = keys.remove(ALL_OUTPUTS) {
        let outputs = live_outputs();
        for output in outputs {
            keys.entry(output).or_insert_with(|| key.clone());
        }
    }
    keys
}

fn live_outputs() -> Vec<String> {
    let mut outputs = skwd_wall_core::outputs::names();
    if outputs.is_empty() {
        outputs.push(ALL_OUTPUTS.to_owned());
    }
    outputs.sort();
    outputs
}

fn location_key(config: &skwd_wall_core::config::Config) -> String {
    wall_proto::weather_location_key(&config.locale(), config.latitude(), config.longitude())
}

fn conditions(forecast: &Forecast) -> Option<Conditions<'_>> {
    if forecast.weather.is_empty() {
        return None;
    }
    let observation = forecast.observation.as_ref()?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    let elapsed =
        if forecast.stale { 0 } else { now.saturating_sub(forecast.fetched_at.unwrap_or(now)) };
    let epoch = observation.observed_at.unwrap_or(forecast.fetched_at.unwrap_or(now) as i64)
        + elapsed as i64;
    let local = epoch + observation.utc_offset_seconds;
    let mut date: libc::tm = unsafe { std::mem::zeroed() };
    unsafe {
        libc::gmtime_r(&raw const local, &raw mut date);
    }
    Some(Conditions {
        latitude: observation.latitude,
        longitude: observation.longitude,
        month: (date.tm_mon + 1) as u32,
        day_of_year: (date.tm_yday + 1) as u32,
        utc_minute: epoch.rem_euclid(MINUTES_PER_DAY * SECONDS_PER_MINUTE) as f64
            / SECONDS_PER_MINUTE as f64,
        temperature: observation.temperature,
        snow_depth: observation.snow_depth,
        is_day: observation.is_day,
        weather: &forecast.weather,
    })
}

fn context(forecast: &Forecast) -> Option<Context> {
    conditions(forecast).as_ref().map(Context::from_conditions)
}

fn candidate_items(
    state: &Arc<WallState>,
    context: &Context,
    resolve: bool,
) -> (Vec<WallpaperItem>, OutdoorMatch) {
    let config = state.config().clone();
    let keys = crate::infrastructure::semantic_index::outdoors_search(state, context, resolve)
        .and_then(Result::ok)
        .unwrap_or_default();
    let mut by_key = BTreeMap::new();
    if !keys.is_empty() {
        let requested: BTreeSet<_> = keys.iter().map(String::as_str).collect();
        let visible = super::profile::Visibility::load(state);
        if state
            .with_db(|connection| {
                db::visit_wallpapers(connection, false, |row| {
                    if row
                        .get(wall_proto::rpc_params::KEY)
                        .and_then(Value::as_str)
                        .is_some_and(|key| requested.contains(key))
                        && visible.shows_item(&row)
                        && let Ok(item) = serde_json::from_value::<WallpaperItem>(row)
                        && let Some(key) = item.key.clone()
                    {
                        by_key.insert(key, item);
                    }
                })
            })
            .is_err()
        {
            by_key.clear();
        }
    }
    let excluded =
        lock(&RUNTIME).as_ref().map(|runtime| runtime.session.excluded.clone()).unwrap_or_default();
    let mut items: Vec<_> = keys
        .iter()
        .filter(|key| !excluded.contains(*key))
        .filter_map(|key| by_key.get(key).cloned())
        .take(config.weather_semantic_max_results())
        .collect();
    if !items.is_empty() {
        let anchor = items
            .iter()
            .find(|item| item.favourite.is_some_and(|favourite| favourite != 0))
            .or_else(|| items.first())
            .and_then(|item| item.hue);
        if let Some(anchor) = anchor {
            items.retain(|item| {
                item.hue.is_none_or(|hue| {
                    let distance = (hue - anchor).abs().rem_euclid(HUE_CIRCLE);
                    distance.min(HUE_CIRCLE - distance) <= PALETTE_DISTANCE
                })
            });
        }
        return (items, OutdoorMatch::Library);
    }
    let entries = crate::infrastructure::outdoors::entries(&config.cache_dir()).unwrap_or_default();
    let mut matching: Vec<_> = entries
        .iter()
        .filter(|entry| entry.matches(context))
        .filter(|entry| {
            entry.item(&config.cache_dir()).key.as_ref().is_some_and(|key| !excluded.contains(key))
        })
        .collect();
    if matching.is_empty() {
        let mut ambient = context.clone();
        ambient.light = match context.light {
            wall_proto::LightPhase::Dawn => wall_proto::LightPhase::Day,
            wall_proto::LightPhase::Dusk => wall_proto::LightPhase::Night,
            other => other,
        };
        matching = entries
            .iter()
            .filter(|entry| {
                entry.match_kind == OutdoorMatch::Ambient
                    && entry.matches(&ambient)
                    && entry
                        .item(&config.cache_dir())
                        .key
                        .as_ref()
                        .is_some_and(|key| !excluded.contains(key))
            })
            .collect();
    }
    let kind = if matching.iter().any(|entry| entry.match_kind == OutdoorMatch::Starter) {
        OutdoorMatch::Starter
    } else {
        OutdoorMatch::Ambient
    };
    if kind == OutdoorMatch::Ambient
        && matching.iter().any(|entry| entry.match_kind == kind && entry.season == context.season)
    {
        matching.retain(|entry| entry.season == context.season);
    }
    let family =
        matching.iter().find(|entry| entry.match_kind == kind).map(|entry| entry.family.as_str());
    (
        matching
            .into_iter()
            .filter(|entry| entry.match_kind == kind && Some(entry.family.as_str()) == family)
            .map(|entry| entry.item(&config.cache_dir()))
            .collect(),
        kind,
    )
}

fn aspect_change(item: &WallpaperItem, output: &str) -> f64 {
    let Some((width, height)) =
        item.width.zip(item.height).filter(|(width, height)| *width > 0 && *height > 0)
    else {
        return 1.0;
    };
    let Some(monitor) =
        skwd_wall_core::outputs::enumerate().into_iter().find(|monitor| monitor.name == output)
    else {
        return 1.0;
    };
    let (out_width, out_height) = monitor.logical_size();
    if out_width <= 0 || out_height <= 0 {
        return 1.0;
    }
    let ratio = (width as f64 / height as f64) / (f64::from(out_width) / f64::from(out_height));
    ratio.max(ratio.recip())
}

fn output_status(
    state: &Arc<WallState>,
    output: &str,
    key: &str,
    items: &[WallpaperItem],
    held: bool,
) -> OutdoorOutput {
    let item = items
        .iter()
        .find(|item| item.key.as_deref() == Some(key))
        .cloned()
        .or_else(|| {
            crate::infrastructure::outdoors::items(&state.config())
                .into_iter()
                .find(|item| item.key.as_deref() == Some(key))
        })
        .or_else(|| {
            let mut found = None;
            state
                .with_db(|connection| {
                    db::visit_wallpapers(connection, false, |row| {
                        if found.is_none()
                            && row.get(wall_proto::rpc_params::KEY).and_then(Value::as_str)
                                == Some(key)
                        {
                            found = serde_json::from_value::<WallpaperItem>(row).ok();
                        }
                    })
                })
                .ok()?;
            found
        });
    let name = item.as_ref().and_then(|item| item.name.clone()).unwrap_or_else(|| {
        let path = key.split_once(':').map_or(key, |(_, path)| path);
        std::path::Path::new(path).file_stem().unwrap_or_default().to_string_lossy().into_owned()
    });
    OutdoorOutput {
        output: output.to_owned(),
        key: key.to_owned(),
        name,
        preview: item.and_then(|item| item.preview.or(item.thumb)),
        held,
    }
}

fn make_status(config: &skwd_wall_core::config::Config, forecast: &Forecast) -> WeatherStatus {
    let no_location =
        config.locale().trim().is_empty() && config.latitude() == 0.0 && config.longitude() == 0.0;
    WeatherStatus {
        enabled: config.weather_enabled(),
        source: WeatherSource::Outdoors,
        weather: forecast.weather.clone(),
        fetched_at: forecast.fetched_at,
        location_key: Some(location_key(config)),
        status: if !config.weather_enabled() {
            WeatherState::Disabled
        } else if no_location {
            WeatherState::NoLocation
        } else {
            WeatherState::Loading
        },
        ..WeatherStatus::default()
    }
}

pub(crate) fn snapshot(ctx: &Ctx) -> WeatherStatus {
    ctx.state.reload_config();
    let config = ctx.state.config().clone();
    let mut guard = lock(&RUNTIME);
    let runtime = initialized(&mut guard, &ctx.state);
    if runtime.config != config {
        super::weather::reload();
        return make_status(&config, &Forecast::default());
    }
    let mut status = runtime.status.clone();
    if let Some(outdoors) = &mut status.outdoors {
        outdoors.generation = runtime.generation;
        for output in &mut outdoors.outputs {
            output.held = runtime.session.held.contains(&output.output)
                || runtime.session.held.contains(ALL_OUTPUTS);
        }
    }
    status.paused_outputs = runtime.session.held.iter().cloned().collect();
    status.resume_available = !status.paused_outputs.is_empty();
    if status.resume_available && status.status == WeatherState::Active {
        status.status = WeatherState::Paused;
    }
    status
}

fn accept_forecast(
    runtime: &mut Runtime,
    config: &skwd_wall_core::config::Config,
    forecast: &Forecast,
) {
    let previous = runtime.context.clone();
    if location_key(&runtime.config) != location_key(config) {
        runtime.context = None;
        runtime.pending = None;
        runtime.plans.clear();
        runtime.due.clear();
        runtime.session.contexts.clear();
        runtime.legacy_outputs.clear();
    }
    if runtime.config != *config || runtime.forecast.stale != forecast.stale {
        runtime.generation = generation();
    }
    runtime.config = config.clone();
    if forecast.stale {
        runtime.pending = None;
    }
    if let Some(next) = context(forecast).filter(|_| !forecast.stale || runtime.context.is_none()) {
        if runtime.context.as_ref() != Some(&next) && runtime.context.is_some() {
            if runtime.pending.as_ref().is_none_or(|(pending, _)| pending != &next) {
                runtime.pending = Some((next.clone(), Instant::now()));
            }
            if runtime.pending.as_ref().is_some_and(|(_, since)| {
                since.elapsed() >= Duration::from_secs(config.outdoors_stable_seconds())
            }) {
                runtime.context = Some(next);
                runtime.pending = None;
            }
        } else {
            runtime.context = Some(next);
            runtime.pending = None;
        }
    }
    if runtime.context != previous {
        runtime.generation = generation();
    }
    runtime.forecast = forecast.clone();
}

pub(crate) fn refresh(ctx: &Ctx, force: bool) -> WeatherStatus {
    ctx.state.reload_config();
    let config = ctx.state.config().clone();
    let forecast = crate::infrastructure::weather::outdoors_forecast(&config, force);
    let (accepted, revision) = {
        let _apply = ctx.state.apply().lock();
        let mut guard = lock(&RUNTIME);
        let runtime = initialized(&mut guard, &ctx.state);
        if runtime.restore_config.is_some() {
            return runtime.status.clone();
        }
        accept_forecast(runtime, &config, &forecast);
        (runtime.context.clone(), runtime.generation)
    };
    let Some(accepted) = accepted else {
        let mut status = make_status(&config, &forecast);
        if status.status == WeatherState::Loading {
            status.status = WeatherState::Unavailable;
        }
        initialized(&mut lock(&RUNTIME), &ctx.state).status = status;
        return snapshot(ctx);
    };
    let (items, kind) = candidate_items(&ctx.state, &accepted, true);
    ctx.state.reload_config();
    if ctx.state.config().clone() != config {
        super::weather::reload();
        return snapshot(ctx);
    }
    let changed = {
        let mut guard = lock(&RUNTIME);
        let runtime = initialized(&mut guard, &ctx.state);
        if runtime.generation != revision {
            super::weather::reload();
            return runtime.status.clone();
        }
        reconcile(runtime, &ctx.state, &accepted, &forecast, items, kind)
    };
    let status = snapshot(ctx);
    if changed {
        ctx.events.publish(
            ev::WEATHER_CHANGED,
            serde_json::to_value(&status).expect("weather status serialization"),
        );
        super::playlist::reload();
    }
    status
}

fn output_keys(items: &[WallpaperItem], output: &str) -> Vec<String> {
    let keys: Vec<_> = items
        .iter()
        .filter(|item| aspect_change(item, output) <= MAX_ASPECT_CHANGE)
        .filter_map(|item| item.key.clone())
        .collect();
    if !keys.is_empty() {
        return keys;
    }
    items
        .iter()
        .min_by(|left, right| aspect_change(left, output).total_cmp(&aspect_change(right, output)))
        .and_then(|item| item.key.clone())
        .into_iter()
        .collect()
}

fn reconcile(
    runtime: &mut Runtime,
    state: &Arc<WallState>,
    context: &Context,
    forecast: &Forecast,
    items: Vec<WallpaperItem>,
    kind: OutdoorMatch,
) -> bool {
    let mut seen = BTreeSet::new();
    let items: Vec<_> = items
        .into_iter()
        .filter(|item| seen.insert(item.key.clone()))
        .filter(|item| item.key.as_ref().is_some_and(|key| !runtime.session.excluded.contains(key)))
        .collect();
    let keys: Vec<_> = items.iter().filter_map(|item| item.key.clone()).collect();
    let mut status = make_status(&runtime.config, forecast);
    status.category = Some(context.category.clone());
    status.query = Some(context.query());
    status.member_keys.clone_from(&keys);
    status.resolved_count = keys.len();
    let current = current_keys(state);
    let mut plans = BTreeMap::new();
    let mut due = BTreeSet::new();
    let previous_contexts = runtime.session.contexts.clone();
    let mut outdoors = OutdoorStatus {
        season: context.season,
        light: context.light,
        match_kind: kind,
        stale: forecast.stale,
        generation: runtime.generation,
        location: if runtime.config.locale().is_empty() {
            format!("{:.2}, {:.2}", runtime.config.latitude(), runtime.config.longitude())
        } else {
            runtime.config.locale()
        },
        credits: Some(crate::infrastructure::outdoors::credits(&runtime.config.cache_dir())),
        outputs: Vec::new(),
    };
    let mut used: BTreeSet<_> = current
        .iter()
        .filter(|(output, _)| {
            runtime.plans.contains_key(*output)
                || runtime.session.held.contains(*output)
                || runtime.session.held.contains(ALL_OUTPUTS)
        })
        .map(|(_, key)| key.clone())
        .collect();
    for (idx, output) in live_outputs().iter().enumerate() {
        let held =
            runtime.session.held.contains(output) || runtime.session.held.contains(ALL_OUTPUTS);
        if held || forecast.stale {
            if forecast.stale
                && let Some(pool) = runtime.plans.get(output)
            {
                plans.insert(output.clone(), pool.clone());
            }
            if let Some(key) = current.get(output) {
                outdoors.outputs.push(output_status(state, output, key, &items, held));
            }
            continue;
        }
        let keys = output_keys(&items, output);
        if keys.is_empty() {
            continue;
        }
        let chosen = current
            .get(output)
            .filter(|key| keys.contains(key))
            .cloned()
            .or_else(|| keys.iter().find(|key| !used.contains(*key)).cloned())
            .unwrap_or_else(|| keys[idx % keys.len()].clone());
        used.insert(chosen.clone());
        let first = keys.iter().position(|key| key == &chosen).expect("chosen pool key");
        let mut pool = keys.clone();
        pool.rotate_left(first);
        plans.insert(output.clone(), pool);
        let current_key = current.get(output);
        if runtime.needs_change(output, current_key, &chosen, context) {
            due.insert(output.clone());
        }
        let shown = if due.contains(output) { &chosen } else { current_key.unwrap_or(&chosen) };
        outdoors.outputs.push(output_status(state, output, shown, &items, false));
    }
    if runtime.plans != plans || runtime.due != due {
        runtime.generation = generation();
    }
    outdoors.generation = runtime.generation;
    runtime.plans = plans;
    runtime.due = due;
    if runtime.session.contexts != previous_contexts {
        runtime.session.location_key = Some(location_key(&runtime.config));
        if let Err(error) = runtime.session.save(&runtime.path) {
            log::warn!("outdoors: session save failed: {error}");
        }
    }
    status.outdoors = Some(outdoors);
    status.status = if !status.enabled {
        WeatherState::Disabled
    } else if keys.is_empty() {
        WeatherState::Empty
    } else {
        WeatherState::Active
    };
    let changed = runtime.status != status;
    runtime.status = status;
    changed
}

pub(super) fn next_index(
    output: &str,
    keys: &[String],
    cursor: usize,
    current: &BTreeMap<String, String>,
) -> Option<usize> {
    (0..keys.len()).map(|offset| (cursor + offset) % keys.len()).find(|index| {
        let key = &keys[*index];
        current.get(output) != Some(key)
            && !current.iter().any(|(name, chosen)| name != output && chosen == key)
    })
}

pub(crate) fn assignment(output: &str) -> Option<(Option<i64>, u64)> {
    let guard = lock(&RUNTIME);
    let runtime = guard.as_ref()?;
    (active(&runtime.config)
        && !runtime.forecast.stale
        && runtime.plans.contains_key(output)
        && !runtime.session.held.contains(output)
        && !runtime.session.held.contains(ALL_OUTPUTS))
    .then_some((None, runtime.generation))
}

pub(crate) fn keys(output: &str) -> Vec<String> {
    lock(&RUNTIME)
        .as_ref()
        .and_then(|runtime| runtime.plans.get(output).cloned())
        .unwrap_or_default()
}

pub(crate) fn due(output: &str) -> bool {
    lock(&RUNTIME).as_ref().is_some_and(|runtime| runtime.due.contains(output))
}

pub(crate) fn allows(state: &Arc<WallState>, output: &str, revision: u64, key: &str) -> bool {
    if state
        .with_db(db::playlist_assigns)
        .unwrap_or_default()
        .iter()
        .any(|(name, _)| name == output || name == ALL_OUTPUTS)
    {
        return false;
    }
    let config = state.config().clone();
    let guard = lock(&RUNTIME);
    let Some(runtime) = guard.as_ref() else { return false };
    if !active(&config)
        || runtime.forecast.stale
        || runtime.config != config
        || runtime.generation != revision
        || runtime.session.held.contains(output)
        || runtime.session.held.contains(ALL_OUTPUTS)
        || runtime.session.excluded.contains(key)
        || runtime.plans.get(output).is_none_or(|keys| !keys.iter().any(|item| item == key))
    {
        return false;
    }
    let builtin = crate::infrastructure::outdoors::items(&config)
        .iter()
        .any(|item| item.key.as_deref() == Some(key));
    drop(guard);
    builtin
        || super::weather::visible_members(state, &[key.to_owned()]).iter().any(|item| item == key)
}

pub(crate) struct ManualChoice<'a> {
    state: &'a Arc<WallState>,
    prior: Option<Runtime>,
}

impl<'a> ManualChoice<'a> {
    pub(crate) fn begin(state: &'a Arc<WallState>, source: ApplySource) -> anyhow::Result<Self> {
        let mut choice = Self { state, prior: None };
        if !source.manual_choice() || !active(&state.config()) {
            return Ok(choice);
        }
        let mut guard = lock(&RUNTIME);
        let runtime = initialized(&mut guard, state);
        let mut stopped = runtime.session.clone();
        stopped.active = false;
        stopped.starting = false;
        stopped.save(&runtime.path)?;
        if let Err(error) = save_config(state, &[(weather::ENABLED.to_owned(), Value::Bool(false))])
        {
            runtime.session.save(&runtime.path)?;
            return Err(error);
        }
        choice.prior = guard.replace(load(state));
        Ok(choice)
    }

    pub(crate) fn commit(mut self, publisher: &dyn EventPublisher) {
        if self.prior.take().is_none() {
            return;
        }
        publisher.publish(
            ev::CONFIG_CHANGED,
            serde_json::to_value(ev::ConfigChanged { library_changed: false })
                .expect("config event serialization"),
        );
        super::weather::reload();
        super::playlist::reload();
        super::schedule::reload();
    }
}

impl Drop for ManualChoice<'_> {
    fn drop(&mut self) {
        let Some(mut prior) = self.prior.take() else { return };
        let mut guard = lock(&RUNTIME);
        if let Err(error) = prior.session.save(&prior.path) {
            log::warn!("outdoors: manual rollback failed: {error}");
            return;
        }
        if let Err(error) =
            save_config(self.state, &[(weather::ENABLED.to_owned(), Value::Bool(true))])
        {
            log::warn!("outdoors: manual rollback failed: {error}");
            if let Some(runtime) = guard.as_ref()
                && let Err(error) = runtime.session.save(&runtime.path)
            {
                log::warn!("outdoors: session rollback failed: {error}");
            }
            return;
        }
        prior.generation = generation();
        *guard = Some(prior);
        super::weather::reload();
        super::playlist::reload();
    }
}

pub(crate) fn note_apply(
    state: &Arc<WallState>,
    source: ApplySource,
    outputs: &[String],
    key: Option<&str>,
) {
    if !active(&state.config()) {
        return;
    }
    let mut guard = lock(&RUNTIME);
    let runtime = initialized(&mut guard, state);
    if matches!(source, ApplySource::WeatherSemantic { .. }) {
        let Some(key) = key else {
            return;
        };
        for output in outputs {
            runtime.session.owned.insert(output.clone(), key.to_owned());
            if let Some(context) = &runtime.context {
                runtime.session.contexts.insert(output.clone(), context.clone());
                runtime.session.location_key = Some(location_key(&runtime.config));
            }
            runtime.due.remove(output);
        }
    } else if matches!(
        source,
        ApplySource::Schedule | ApplySource::Workspace | ApplySource::Playlist
    ) {
        runtime.session.held.extend(outputs.iter().cloned());
        runtime.generation = generation();
    } else {
        return;
    }
    if let Err(error) = runtime.session.save(&runtime.path) {
        log::warn!("outdoors: session save failed: {error}");
    }
    super::weather::reload();
}

pub(crate) fn cancel_restore() {
    let mut guard = lock(&RUNTIME);
    if let Some(runtime) = guard.as_mut()
        && runtime.restore_config.take().is_some()
    {
        runtime.generation = generation();
    }
}

pub(crate) fn wait() -> Duration {
    let guard = lock(&RUNTIME);
    let Some(runtime) = guard.as_ref() else {
        return crate::infrastructure::weather::refresh_interval();
    };
    if runtime.forecast.stale {
        return crate::infrastructure::weather::refresh_interval();
    }
    let light = conditions(&runtime.forecast)
        .map_or_else(crate::infrastructure::weather::refresh_interval, |conditions| {
            Duration::from_secs(wall_rules::outdoors::next_light_seconds(&conditions))
        });
    runtime
        .pending
        .as_ref()
        .map_or(light, |(_, since)| {
            Duration::from_secs(runtime.config.outdoors_stable_seconds())
                .saturating_sub(since.elapsed())
                .min(light)
        })
        .max(Duration::from_secs(1))
}

pub(crate) fn follow(ctx: &Ctx, enabled: bool) -> anyhow::Result<WeatherStatus> {
    let _control = lock(&CONTROL);
    let apply_guard = ctx.state.apply().lock();
    ctx.state.reload_config();
    let mut session = {
        let mut guard = lock(&RUNTIME);
        let runtime = initialized(&mut guard, &ctx.state);
        if enabled && runtime.session.active && active(&ctx.state.config()) {
            return Ok(runtime.status.clone());
        }
        runtime.generation = generation();
        runtime.plans.clear();
        runtime.due.clear();
        if enabled {
            let mut captured = Session::capture(&ctx.state)?;
            captured.excluded.clone_from(&runtime.session.excluded);
            captured
        } else {
            runtime.session.clone()
        }
    };
    let path = PathBuf::from(ctx.state.config().cache_dir()).join(SESSION_FILE);
    let mut restore = BTreeMap::new();
    if enabled {
        session.save(&path)?;
        let result = ctx
            .state
            .with_db(|connection| {
                let transaction = connection.unchecked_transaction()?;
                db::playlist_assign_clear(&transaction, None)?;
                if let Err(error) = save_config(&ctx.state, &session.applied_settings) {
                    transaction.rollback()?;
                    return Ok(Err(error));
                }
                transaction.commit()?;
                Ok(Ok(()))
            })
            .map_err(anyhow::Error::from)
            .and_then(std::convert::identity);
        if let Err(error) = result {
            save_config(&ctx.state, &session.settings)?;
            session.active = false;
            session.starting = false;
            session.save(&path)?;
            return Err(error);
        }
        session.starting = false;
        session.save(&path)?;
    } else if session.active {
        session.release_empty(ctx)?;
        session.restore_settings(&ctx.state)?;
        session.restore_assignments(&ctx.state)?;
        restore.clone_from(&session.wallpapers);
        let mut guard = lock(&RUNTIME);
        initialized(&mut guard, &ctx.state).restore_config = Some(ctx.state.config().clone());
        session.active = false;
        session.save(&path)?;
    } else if ctx.state.config().weather_source() == WeatherSource::Outdoors {
        save_config(
            &ctx.state,
            &[
                (weather::SOURCE.to_owned(), Value::String(weather::SOURCE_PLAYLISTS.to_owned())),
                (weather::ENABLED.to_owned(), Value::Bool(false)),
            ],
        )?;
    }
    let revision = {
        let mut guard = lock(&RUNTIME);
        let restore_config = guard.as_mut().and_then(|runtime| runtime.restore_config.take());
        let mut runtime = load(&ctx.state);
        runtime.session = session;
        runtime.restore_config = restore_config;
        guard.insert(runtime).generation
    };
    drop(apply_guard);
    for (output, key) in restore {
        crate::composition::apply::apply_by_key(
            &ctx.state,
            ctx.wallpaper.as_ref(),
            ctx.history.as_ref(),
            ctx.events.as_ref(),
            &ctx.stats,
            &key,
            &output,
            false,
            ApplySource::OutdoorsRestore { revision },
        );
    }
    {
        let _apply = ctx.state.apply().lock();
        cancel_restore();
    }
    ctx.events.publish(
        ev::CONFIG_CHANGED,
        serde_json::to_value(ev::ConfigChanged { library_changed: true })?,
    );
    super::playlist::reload();
    super::schedule::reload();
    crate::infrastructure::semantic_index::settings_changed();
    Ok(if enabled { snapshot(ctx) } else { super::weather::snapshot(ctx) })
}

pub(crate) fn feedback(
    ctx: &Ctx,
    method: &str,
    output: &str,
    key: Option<&str>,
) -> anyhow::Result<WeatherStatus> {
    let _control = lock(&CONTROL);
    let _apply = ctx.state.apply().lock();
    ctx.state.reload_config();
    anyhow::ensure!(active(&ctx.state.config()), FOLLOW_DISABLED);
    anyhow::ensure!(
        output == ALL_OUTPUTS || live_outputs().iter().any(|name| name == output),
        UNKNOWN_OUTPUT
    );
    let current = current_keys(&ctx.state);
    {
        let mut guard = lock(&RUNTIME);
        let runtime = initialized(&mut guard, &ctx.state);
        runtime.legacy_outputs.clear();
        match method {
            wall_proto::rpc::WALL_WEATHER_EXCLUDE => {
                let key = key
                    .filter(|key| !key.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("wallpaper key required"))?;
                runtime.session.excluded.insert(key.to_owned());
                for (name, current_key) in &current {
                    if current_key == key {
                        runtime.session.contexts.remove(name);
                    }
                }
                runtime.status.member_keys.retain(|item| item != key);
                runtime.status.resolved_count = runtime.status.member_keys.len();
                for pool in runtime.plans.values_mut() {
                    pool.retain(|item| item != key);
                }
            }
            wall_proto::rpc::WALL_WEATHER_PIN => {
                runtime.session.held.insert(output.to_owned());
            }
            wall_proto::rpc::WALL_WEATHER_RESUME => {
                if output == ALL_OUTPUTS {
                    runtime.session.held.clear();
                    runtime.session.contexts.clear();
                } else {
                    if runtime.session.held.remove(ALL_OUTPUTS) {
                        runtime
                            .session
                            .held
                            .extend(live_outputs().into_iter().filter(|name| name != output));
                    }
                    runtime.session.held.remove(output);
                    runtime.session.contexts.remove(output);
                }
            }
            wall_proto::rpc::WALL_WEATHER_NEXT => {
                anyhow::ensure!(!runtime.forecast.stale, STALE_WEATHER);
            }
            _ => anyhow::bail!("unknown outdoors action"),
        }
        if method != wall_proto::rpc::WALL_WEATHER_NEXT {
            runtime.generation = generation();
        }
        runtime.session.save(&runtime.path)?;
    }
    if method == wall_proto::rpc::WALL_WEATHER_NEXT {
        super::playlist::command(&ctx.state, output, true);
    }
    super::playlist::reload();
    let status = snapshot(ctx);
    ctx.events.publish(ev::WEATHER_CHANGED, serde_json::to_value(&status)?);
    Ok(status)
}

#[cfg(test)]
mod tests;

pub(crate) fn deactivate(config: &skwd_wall_core::config::Config) {
    let mut guard = lock(&RUNTIME);
    if let Some(runtime) = guard.as_mut() {
        if runtime.restore_config.is_some() {
            return;
        }
        runtime.config = config.clone();
        runtime.plans.clear();
        runtime.due.clear();
        runtime.generation = generation();
    }
}

pub(crate) fn restore_allows(
    state: &Arc<WallState>,
    output: &str,
    revision: u64,
    key: &str,
) -> bool {
    state.reload_config();
    let current = current_keys(state);
    let guard = lock(&RUNTIME);
    let Some(runtime) = guard.as_ref() else {
        return false;
    };
    runtime.generation == revision
        && runtime.restore_config.as_ref() == Some(&state.config().clone())
        && !runtime.session.held.contains(output)
        && !runtime.session.held.contains(ALL_OUTPUTS)
        && runtime.session.wallpapers.get(output).is_some_and(|prior| prior == key)
        && runtime.session.owned.get(output).is_some_and(|owned| current.get(output) == Some(owned))
}
