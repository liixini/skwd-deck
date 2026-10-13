use skwd_wall_core::lock;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value;
use skwd_wall_core::{WallState, db, outputs};
use tokio::sync::Notify;

use crate::backend::history::ApplySource;
use crate::composition::runtime::profile::Visibility;
use crate::infrastructure::wake::wake_or_timeout;

const PLAYLIST_ITEM_KEY: &str = "key";
const PLAYLIST_ITEM_TYPE: &str = "type";
use wall_rules::playlist::{
    Filter, Order, initial_index, parse_order, source_wants_favourites, step,
};

const MIN_DWELL: Duration = Duration::from_secs(60);
const MIN_TICK_WAIT: Duration = Duration::from_millis(50);

struct Def {
    kind: String,
    source: String,
    order: Order,
    dwell: Duration,
}

struct OutputState {
    playlist_id: Option<i64>,
    weather_revision: Option<u64>,
    keys: Vec<String>,
    order: Order,
    dwell: Duration,
    cursor: usize,
    next_fire: Instant,
    event_only: bool,
    armed: bool,
}

impl OutputState {
    fn retry_weather(&mut self, revision: u64, now: Instant) -> bool {
        if self.weather_revision != Some(revision) {
            return false;
        }
        self.next_fire = now + MIN_DWELL;
        self.armed = true;
        true
    }
}

struct Runtime {
    outputs: HashMap<String, OutputState>,
    rng: u64,
}

pub struct Engine {
    rt: Mutex<Runtime>,
    wake: Notify,
}

static ENGINE: OnceLock<Arc<Engine>> = OnceLock::new();

pub fn assignments_active(state: &Arc<WallState>) -> bool {
    state.config().weather_enabled()
        || state.with_db(db::playlist_assigns).is_ok_and(|assigns| !assigns.is_empty())
}

pub fn reload() {
    super::weather::reload();
    if let Some(engine) = ENGINE.get() {
        engine.wake.notify_one();
    }
}

pub fn command(state: &Arc<WallState>, output: &str, forward: bool) -> bool {
    let Some(engine) = ENGINE.get() else {
        return false;
    };
    let mut rt = lock(&engine.rt);
    let any = command_runtime(&mut rt, state, output, forward, Instant::now());
    drop(rt);
    engine.wake.notify_one();
    any
}

fn command_runtime(
    rt: &mut Runtime,
    state: &Arc<WallState>,
    output: &str,
    forward: bool,
    now: Instant,
) -> bool {
    reconcile(rt, state);
    let targets: Vec<String> =
        if output == "*" { rt.outputs.keys().cloned().collect() } else { vec![output.to_string()] };
    let current =
        super::outdoors::active(&state.config()).then(|| super::outdoors::current_keys(state));
    let mut any = false;
    let mut rng = rt.rng;
    for name in targets {
        if let Some(st) = rt.outputs.get_mut(&name) {
            if forward
                && let Some(current) = &current
                && let Some(index) =
                    super::outdoors::next_index(&name, &st.keys, st.cursor, current)
            {
                st.cursor = index;
            }
            queue_command(st, forward, &mut rng, now);
            any = true;
        }
    }
    rt.rng = rng;
    any
}

fn queue_command(st: &mut OutputState, forward: bool, rng: &mut u64, now: Instant) {
    if !forward && st.keys.len() > 1 {
        let steps = if st.order == Order::Sequential { 2 } else { 1 };
        for _ in 0..steps {
            st.cursor = step(st.order, st.cursor, st.keys.len(), false, rng);
        }
    }
    st.next_fire = now;
    st.armed = true;
}

fn load_defs(state: &Arc<WallState>) -> HashMap<i64, Def> {
    state
        .with_db(db::playlists_all)
        .unwrap_or_default()
        .into_iter()
        .map(|row| {
            let dwell_s = u64::try_from(row.dwell).unwrap_or(600).max(5);
            (
                row.id,
                Def {
                    kind: row.kind,
                    source: row.source.unwrap_or_else(|| "all".to_string()),
                    order: parse_order(&row.order),
                    dwell: Duration::from_secs(dwell_s),
                },
            )
        })
        .collect()
}

pub fn resolve_member_items(state: &Arc<WallState>, id: i64) -> Vec<Value> {
    let defs = load_defs(state);
    defs.get(&id).map(|def| member_items(state, id, def)).unwrap_or_default()
}

fn member_items(state: &Arc<WallState>, id: i64, def: &Def) -> Vec<Value> {
    if def.kind == "smart" {
        let fav = source_wants_favourites(&def.source);
        let items = state.with_db(|conn| db::list_wallpapers(conn, fav)).unwrap_or_default();
        let visibility = Visibility::load(state);
        let filter = Filter::parse(&def.source);
        items.into_iter().filter(|item| visibility.matches(item, &filter)).collect()
    } else {
        state.with_db(|conn| db::playlist_member_items(conn, id)).unwrap_or_default()
    }
}

pub fn list_with_resolved_counts(state: &Arc<WallState>) -> Vec<wall_proto::PlaylistRow> {
    let mut rows = state.with_db(db::playlists_all).unwrap_or_default();
    if !rows.iter().any(|row| row.kind == "smart") {
        return rows;
    }

    let visibility = Visibility::load(state);
    let all_items = state.with_db(|conn| db::list_wallpapers(conn, false)).unwrap_or_default();
    let favourite_items = rows
        .iter()
        .filter(|row| row.kind == "smart")
        .filter_map(|row| row.source.as_deref())
        .any(source_wants_favourites)
        .then(|| state.with_db(|conn| db::list_wallpapers(conn, true)).unwrap_or_default());

    for row in &mut rows {
        if row.kind != "smart" {
            continue;
        }
        let source = row.source.as_deref().unwrap_or(wall_proto::playlist_source::ALL);
        let filter = Filter::parse(source);
        let items = if source_wants_favourites(source) {
            favourite_items.as_deref().unwrap_or_default()
        } else {
            &all_items
        };
        row.count = items
            .iter()
            .filter(|item| visibility.matches(item, &filter) && visibility.shows_item(item))
            .count() as i64;
    }
    rows
}

pub(crate) fn resolve_member_keys(state: &Arc<WallState>, id: i64) -> Vec<String> {
    let items: Vec<_> = resolve_member_items(state, id)
        .into_iter()
        .filter(|item| item.get(PLAYLIST_ITEM_TYPE).and_then(Value::as_str).is_some())
        .collect();
    visible_keys(state, &items)
}

fn resolve_keys(state: &Arc<WallState>, id: i64, def: &Def) -> Vec<String> {
    visible_keys(state, &member_items(state, id, def))
}

fn visible_keys(state: &Arc<WallState>, items: &[Value]) -> Vec<String> {
    let visibility = Visibility::load(state);
    items
        .iter()
        .filter(|item| visibility.shows_item(item))
        .filter_map(|item| item.get(PLAYLIST_ITEM_KEY).and_then(Value::as_str).map(str::to_owned))
        .collect()
}

fn effective(assigns: &[(String, i64)], output: &str) -> Option<i64> {
    assigns
        .iter()
        .find(|(out, _)| out == output)
        .or_else(|| assigns.iter().find(|(out, _)| out == "*"))
        .map(|(_, id)| *id)
}

type Assignments = HashMap<String, (Option<i64>, Option<u64>)>;

fn assignments(state: &Arc<WallState>, defs: &HashMap<i64, Def>) -> Assignments {
    let assigns = state.with_db(db::playlist_assigns).unwrap_or_default();
    let mut live = outputs::names();
    if live.is_empty() {
        live.push("*".to_string());
    }
    live.iter()
        .filter_map(|out| {
            effective(&assigns, out)
                .map(|id| (Some(id), None))
                .or_else(|| {
                    state
                        .config()
                        .weather_enabled()
                        .then(|| {
                            if super::outdoors::active(&state.config()) {
                                super::outdoors::assignment(out)
                            } else {
                                super::weather::assignment(out)
                            }
                        })
                        .flatten()
                        .map(|(id, revision)| (id, Some(revision)))
                })
                .filter(|(id, revision)| {
                    revision.is_some() && id.is_none()
                        || id.is_some_and(|id| defs.contains_key(&id))
                })
                .map(|assignment| (out.clone(), assignment))
        })
        .collect()
}

fn reconcile(rt: &mut Runtime, state: &Arc<WallState>) {
    let defs = load_defs(state);
    let outdoors = super::outdoors::active(&state.config());
    let current = if outdoors {
        super::outdoors::current_keys(state)
    } else {
        std::collections::BTreeMap::new()
    };
    let now = Instant::now();
    let wanted = assignments(state, &defs);
    rt.outputs.retain(|out, _| wanted.contains_key(out));
    for (out, (id, weather_revision)) in wanted {
        let event_only = outdoors && id.is_none() && weather_revision.is_some();
        let due = event_only && super::outdoors::due(&out);
        let (keys, order, dwell) = match id {
            None => (
                super::weather::keys(&out, outdoors),
                if outdoors { Order::Sequential } else { Order::Shuffle },
                if event_only {
                    Duration::ZERO
                } else {
                    Duration::from_secs(state.config().weather_semantic_dwell_seconds())
                },
            ),
            Some(id) => {
                let def = &defs[&id];
                let keys = if weather_revision.is_some() {
                    super::weather::member_keys()
                } else {
                    resolve_keys(state, id, def)
                };
                (keys, def.order, def.dwell)
            }
        };
        if keys.is_empty() {
            log::warn!("playlist: '{out}' -> {id:?} resolved 0 wallpapers; will not cycle");
        }
        match rt.outputs.get_mut(&out) {
            Some(st)
                if st.playlist_id == id
                    && st.weather_revision.is_some() == weather_revision.is_some() =>
            {
                if weather_revision.is_some() && st.weather_revision != weather_revision {
                    st.cursor =
                        if outdoors { 0 } else { initial_index(order, keys.len(), &mut rt.rng) };
                    if !event_only || due && current.get(&out) != keys.first() {
                        st.next_fire = now;
                        st.armed = true;
                    } else {
                        st.cursor = usize::from(keys.len() > 1);
                        st.armed = false;
                    }
                }
                st.weather_revision = weather_revision;
                st.order = order;
                st.dwell = dwell;
                st.event_only = event_only;
                if st.keys.is_empty() {
                    st.cursor = initial_index(order, keys.len(), &mut rt.rng);
                    if weather_revision.is_some() && !keys.is_empty() && (!event_only || due) {
                        st.next_fire = now;
                        st.armed = true;
                    }
                } else if !keys.is_empty() {
                    st.cursor = st.cursor.min(keys.len() - 1);
                }
                st.keys = keys;
            }
            _ => {
                log::info!(
                    "playlist: '{out}' -> {id:?} ({} items, dwell {}s)",
                    keys.len(),
                    dwell.as_secs()
                );
                rt.outputs.insert(
                    out,
                    OutputState {
                        playlist_id: id,
                        weather_revision,
                        cursor: if outdoors {
                            0
                        } else {
                            initial_index(order, keys.len(), &mut rt.rng)
                        },
                        keys,
                        order,
                        dwell,
                        next_fire: if weather_revision.is_some() { now } else { now + dwell },
                        event_only,
                        armed: !event_only || due,
                    },
                );
            }
        }
    }
}

pub fn start(ctx: crate::composition::context::Ctx) {
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0x9e3779b97f4a7c15, |dur| dur.as_nanos() as u64 | 1);
    let engine = Arc::new(Engine {
        rt: Mutex::new(Runtime { outputs: HashMap::new(), rng: seed }),
        wake: Notify::new(),
    });
    let _ = ENGINE.set(Arc::clone(&engine));
    tokio::spawn(run(engine, ctx));
}

type QueuedApply = (String, String, ApplySource);

fn tick(rt: &mut Runtime, now: Instant, long: Duration) -> (Vec<QueuedApply>, Duration) {
    let mut applies = Vec::new();
    let mut rng = rt.rng;
    for (name, st) in &mut rt.outputs {
        if !st.armed || st.next_fire > now {
            continue;
        }
        if st.keys.is_empty() {
            st.next_fire = now + st.dwell.max(MIN_DWELL);
            continue;
        }
        st.cursor = st.cursor.min(st.keys.len() - 1);
        let key = st.keys[st.cursor].clone();
        if st.event_only {
            log::info!("playlist: fire '{name}' -> {key}");
        } else {
            log::info!("playlist: fire '{name}' -> {key} (next in {}s)", st.dwell.as_secs());
        }
        st.cursor = step(st.order, st.cursor, st.keys.len(), true, &mut rng);
        st.next_fire = now + st.dwell;
        st.armed = !st.event_only;
        let source = match (st.playlist_id, st.weather_revision) {
            (Some(playlist_id), Some(revision)) => ApplySource::Weather { playlist_id, revision },
            (None, Some(revision)) => ApplySource::WeatherSemantic { revision },
            _ => ApplySource::Playlist,
        };
        applies.push((name.clone(), key, source));
    }
    rt.rng = rng;
    let wait = rt
        .outputs
        .values()
        .filter(|st| st.armed)
        .map(|st| st.next_fire.saturating_duration_since(now))
        .min()
        .unwrap_or(long)
        .clamp(MIN_TICK_WAIT, long);
    (applies, wait)
}

async fn run(engine: Arc<Engine>, ctx: crate::composition::context::Ctx) {
    let long = crate::composition::bootstrap::IDLE_RECHECK;
    loop {
        let tick_engine = Arc::clone(&engine);
        let tick_state = Arc::clone(&ctx.state);
        let Ok((to_apply, wait)) = tokio::task::spawn_blocking(move || {
            tick_state.reload_config();
            let mut rt = lock(&tick_engine.rt);
            reconcile(&mut rt, &tick_state);
            tick(&mut rt, Instant::now(), long)
        })
        .await
        else {
            break;
        };
        if !to_apply.is_empty() {
            let task = ctx.clone();
            let retry = Arc::clone(&engine);
            let _ = tokio::task::spawn_blocking(move || {
                for (out, key, source) in to_apply {
                    task.stats.set_task("playlist");
                    let applied = crate::composition::apply::apply_by_key(
                        &task.state,
                        task.wallpaper.as_ref(),
                        task.history.as_ref(),
                        task.events.as_ref(),
                        &task.stats,
                        &key,
                        &out,
                        false,
                        source,
                    );
                    if !applied
                        && let ApplySource::WeatherSemantic { revision } = source
                        && super::outdoors::due(&out)
                    {
                        let mut rt = lock(&retry.rt);
                        if let Some(st) = rt.outputs.get_mut(&out)
                            && st.retry_weather(revision, Instant::now())
                        {
                            retry.wake.notify_one();
                        }
                    }
                    task.stats.set_task("idle");
                }
                skwd_wall_core::matugen::notify_change(&task.state.config());
            })
            .await;
        }
        wake_or_timeout(&engine.wake, wait).await;
    }
}

mod tests;
