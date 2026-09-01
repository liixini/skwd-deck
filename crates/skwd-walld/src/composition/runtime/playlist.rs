use skwd_wall_core::lock;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value;
use skwd_wall_core::{WallState, db, outputs};
use tokio::sync::Notify;

use crate::backend::history::ApplySource;
use crate::infrastructure::wake::wake_or_timeout;
use wall_rules::playlist::{Order, matches_item, parse_order, source_wants_favourites, step};

const MIN_DWELL: Duration = Duration::from_secs(60);
const MIN_TICK_WAIT: Duration = Duration::from_millis(50);

struct Def {
    kind: String,
    source: String,
    order: Order,
    dwell: Duration,
}

struct OutputState {
    playlist_id: i64,
    keys: Vec<String>,
    order: Order,
    dwell: Duration,
    cursor: usize,
    next_fire: Instant,
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
    state.with_db(db::playlist_assigns).is_ok_and(|assigns| !assigns.is_empty())
}

pub fn reload() {
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
    let mut any = false;
    let mut rng = rt.rng;
    for name in targets {
        if let Some(st) = rt.outputs.get_mut(&name) {
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
    let Some(def) = defs.get(&id) else {
        return Vec::new();
    };
    if def.kind == "smart" {
        let fav = source_wants_favourites(&def.source);
        let items = state.with_db(|conn| db::list_wallpapers(conn, fav)).unwrap_or_default();
        items.into_iter().filter(|item| matches_item(item, &def.source)).collect()
    } else {
        state.with_db(|conn| db::playlist_member_items(conn, id)).unwrap_or_default()
    }
}

pub fn list_with_resolved_counts(state: &Arc<WallState>) -> Vec<wall_proto::PlaylistRow> {
    let mut rows = state.with_db(db::playlists_all).unwrap_or_default();
    if !rows.iter().any(|row| row.kind == "smart") {
        return rows;
    }

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
        let source = row.source.as_deref().unwrap_or("all");
        let items = if source_wants_favourites(source) {
            favourite_items.as_deref().unwrap_or_default()
        } else {
            &all_items
        };
        row.count = items.iter().filter(|item| matches_item(item, source)).count() as i64;
    }
    rows
}

fn resolve_keys(state: &Arc<WallState>, id: i64, def: &Def) -> Vec<String> {
    if def.kind == "smart" {
        let fav = source_wants_favourites(&def.source);
        let items = state.with_db(|conn| db::list_wallpapers(conn, fav)).unwrap_or_default();
        items
            .iter()
            .filter_map(|item| {
                let key = item.get("key").and_then(Value::as_str)?.to_string();
                matches_item(item, &def.source).then_some(key)
            })
            .collect()
    } else {
        state.with_db(|conn| db::playlist_members(conn, id)).unwrap_or_default()
    }
}

fn effective(assigns: &[(String, i64)], output: &str) -> Option<i64> {
    assigns
        .iter()
        .find(|(out, _)| out == output)
        .or_else(|| assigns.iter().find(|(out, _)| out == "*"))
        .map(|(_, id)| *id)
}

fn reconcile(rt: &mut Runtime, state: &Arc<WallState>) {
    let defs = load_defs(state);
    let assigns = state.with_db(db::playlist_assigns).unwrap_or_default();
    let mut live = outputs::names();
    if live.is_empty() {
        live.push("*".to_string());
    }
    let now = Instant::now();
    let wanted: HashMap<String, i64> = live
        .iter()
        .filter_map(|out| {
            effective(&assigns, out).filter(|id| defs.contains_key(id)).map(|id| (out.clone(), id))
        })
        .collect();
    rt.outputs.retain(|out, _| wanted.contains_key(out));
    for (out, id) in wanted {
        let def = &defs[&id];
        let keys = resolve_keys(state, id, def);
        if keys.is_empty() {
            log::warn!(
                "playlist: '{out}' -> id {id} ({}:{}) resolved 0 wallpapers; will not cycle",
                def.kind,
                def.source
            );
        }
        match rt.outputs.get_mut(&out) {
            Some(st) if st.playlist_id == id => {
                st.order = def.order;
                st.dwell = def.dwell;
                if !keys.is_empty() {
                    st.cursor = st.cursor.min(keys.len() - 1);
                }
                st.keys = keys;
            }
            _ => {
                log::info!(
                    "playlist: '{out}' -> id {id} ({} items, dwell {}s)",
                    keys.len(),
                    def.dwell.as_secs()
                );
                rt.outputs.insert(
                    out,
                    OutputState {
                        playlist_id: id,
                        keys,
                        order: def.order,
                        dwell: def.dwell,
                        cursor: 0,
                        next_fire: now + def.dwell,
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

fn tick(rt: &mut Runtime, now: Instant, long: Duration) -> (Vec<(String, String)>, Duration) {
    let mut applies = Vec::new();
    let mut rng = rt.rng;
    for (name, st) in &mut rt.outputs {
        if st.next_fire > now {
            continue;
        }
        if st.keys.is_empty() {
            st.next_fire = now + st.dwell.max(MIN_DWELL);
            continue;
        }
        st.cursor = st.cursor.min(st.keys.len() - 1);
        let key = st.keys[st.cursor].clone();
        log::info!("playlist: fire '{name}' -> {key} (next in {}s)", st.dwell.as_secs());
        st.cursor = step(st.order, st.cursor, st.keys.len(), true, &mut rng);
        st.next_fire = now + st.dwell;
        applies.push((name.clone(), key));
    }
    rt.rng = rng;
    let wait = rt
        .outputs
        .values()
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
            let _ = tokio::task::spawn_blocking(move || {
                for (out, key) in to_apply {
                    task.stats.set_task("playlist");
                    crate::composition::apply::apply_by_key(
                        &task.state,
                        task.wallpaper.as_ref(),
                        task.history.as_ref(),
                        task.events.as_ref(),
                        &task.stats,
                        &key,
                        &out,
                        false,
                        ApplySource::Playlist,
                    );
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
