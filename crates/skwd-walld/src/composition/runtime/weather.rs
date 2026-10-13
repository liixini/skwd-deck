use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use skwd_wall_core::{WallState, db, lock};
use tokio::sync::Notify;
use wall_proto::{
    WeatherCandidates, WeatherSelection, WeatherSource, WeatherState, WeatherStatus, ev,
    weather_category,
};

use crate::backend::events::EventPublisher;
use crate::backend::history::ApplySource;
use crate::composition::context::Ctx;
use crate::infrastructure::weather::{Forecast, cached, forecast, refresh_interval};

const PAUSE_FILE: &str = "weather-pauses.json";
const ALL_OUTPUTS: &str = "*";
type Location = (String, f64, f64);

struct Resolved {
    status: WeatherStatus,
    location: Location,
    config: skwd_wall_core::config::Config,
}

#[derive(Default, Deserialize, Serialize)]
struct Policy {
    category: Option<String>,
    playlist_id: Option<i64>,
    #[serde(default)]
    source: WeatherSource,
    #[serde(default)]
    query: Option<String>,
    paused: BTreeSet<String>,
}

struct Runtime {
    status: WeatherStatus,
    approved: Vec<String>,
    policy: Policy,
    revision: u64,
    path: PathBuf,
    location: Option<Location>,
}

impl Runtime {
    fn load(path: PathBuf) -> Self {
        let policy = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            status: WeatherStatus::default(),
            approved: Vec::new(),
            policy,
            revision: 0,
            path,
            location: None,
        }
    }

    fn save(&self) {
        let bytes = serde_json::to_vec(&self.policy).expect("weather policy serialization");
        if let Err(error) = skwd_config::atomic_write(&self.path, &bytes) {
            log::warn!("weather: pause save failed: {error}");
        }
    }

    fn update(&mut self, mut status: WeatherStatus) -> bool {
        let query = pause_query(status.source, status.selection, status.query.clone());
        if !status.weather.is_empty()
            && (self.policy.category != status.category
                || self.policy.playlist_id != status.playlist_id
                || self.policy.source != status.source
                || self.policy.query != query)
        {
            if self.policy.category.is_some() || self.policy.playlist_id.is_some() {
                self.policy.paused.clear();
            }
            self.policy.category.clone_from(&status.category);
            self.policy.playlist_id = status.playlist_id;
            self.policy.source = status.source;
            self.policy.query = query;
            self.save();
        }
        status.resume_available = !self.policy.paused.is_empty();
        status.paused_outputs.extend(self.policy.paused.iter().cloned());
        status.paused_outputs.sort();
        status.paused_outputs.dedup();
        if status.enabled
            && status.status == WeatherState::Active
            && !status.paused_outputs.is_empty()
        {
            status.status = WeatherState::Paused;
        }
        if self.status == status {
            return false;
        }
        if self.status.enabled != status.enabled
            || self.status.category != status.category
            || self.status.playlist_id != status.playlist_id
            || self.status.source != status.source
            || self.status.selection != status.selection
            || self.status.query != status.query
            || self.status.member_keys != status.member_keys
            || self.status.status != status.status
            || self.status.paused_outputs != status.paused_outputs
        {
            self.revision = self.revision.wrapping_add(1);
        }
        self.status = status;
        true
    }

    fn assignment(&self, output: &str) -> Option<(Option<i64>, u64)> {
        if !self.status.enabled
            || self.status.member_keys.is_empty()
            || !matches!(self.status.status, WeatherState::Active | WeatherState::Paused)
            || self.policy.paused.contains(ALL_OUTPUTS)
            || self.policy.paused.contains(output)
        {
            return None;
        }
        Some((self.status.playlist_id, self.revision))
    }
}

static RUNTIME: Mutex<Option<Runtime>> = Mutex::new(None);
static PUBLISH_LOCK: Mutex<()> = Mutex::new(());
static REFRESH_LOCK: Mutex<()> = Mutex::new(());
static WAKE: Notify = Notify::const_new();

pub(crate) fn reload() {
    WAKE.notify_one();
}

pub(crate) fn assignment(output: &str) -> Option<(Option<i64>, u64)> {
    lock(&RUNTIME).as_ref()?.assignment(output)
}

pub(crate) fn keys(output: &str, outdoors: bool) -> Vec<String> {
    if outdoors { super::outdoors::keys(output) } else { member_keys() }
}

pub(crate) fn member_keys() -> Vec<String> {
    lock(&RUNTIME).as_ref().map(|runtime| runtime.status.member_keys.clone()).unwrap_or_default()
}

pub(crate) fn allows(
    state: &Arc<WallState>,
    output: &str,
    playlist_id: Option<i64>,
    revision: u64,
    key: &str,
) -> bool {
    state.reload_config();
    let config = state.config().clone();
    if config.weather_source() == WeatherSource::Outdoors {
        return super::outdoors::allows(state, output, revision, key);
    }
    if !config.weather_enabled() {
        return false;
    }
    let eligible = lock(&RUNTIME).as_ref().is_some_and(|runtime| {
        runtime.location.as_ref() == Some(&location(&config))
            && runtime.assignment(output) == Some((playlist_id, revision))
            && runtime.status.source == config.weather_source()
            && runtime.status.selection == config.weather_selection()
            && runtime.approved == approval_signature(&config, &runtime.status)
            && selection(&config, &runtime.status.weather).is_some_and(|(category, id, query)| {
                id == playlist_id
                    && query == runtime.status.query
                    && runtime.status.category.as_deref() == Some(category)
            })
            && runtime.status.member_keys.iter().any(|member| member == key)
    });
    if !eligible {
        return false;
    }
    let manual = state.with_db(db::playlist_assigns).unwrap_or_default();
    if manual.iter().any(|(name, _)| name == output || name == ALL_OUTPUTS) {
        return false;
    }
    if let Some(id) = playlist_id {
        super::playlist::resolve_member_keys(state, id).iter().any(|member| member == key)
    } else {
        let Some(category) =
            lock(&RUNTIME).as_ref().and_then(|runtime| runtime.status.category.clone())
        else {
            return false;
        };
        if config.weather_selection() == WeatherSelection::Approved {
            return approved_members(state, &config, &category).iter().any(|member| member == key);
        }
        let Some(query) = config.weather_semantic_query(&category) else { return false };
        config.semantic_enabled()
            && visible_members(state, &[key.to_owned()]).iter().any(|member| member == key)
            && crate::infrastructure::semantic_index::weather_cached_match(
                state,
                &query,
                config.weather_semantic_max_results(),
                config.weather_semantic_score_window(),
                config.weather_semantic_prominence(),
                key,
            )
    }
}

fn selection(
    config: &skwd_wall_core::config::Config,
    weather: &[String],
) -> Option<(&'static str, Option<i64>, Option<String>)> {
    for category in weather_category::PRIORITY {
        if !weather.iter().any(|tag| tag == category) {
            continue;
        }
        match config.weather_source() {
            WeatherSource::Outdoors => return None,
            WeatherSource::Playlists => {
                if let Some(id) = config.weather_playlist(category) {
                    return Some((category, Some(id), None));
                }
            }
            WeatherSource::Semantic => {
                let query = config.weather_semantic_query(category);
                if query.is_some() || config.weather_selection() == WeatherSelection::Approved {
                    return Some((category, None, query));
                }
            }
        }
    }
    None
}

fn location(config: &skwd_wall_core::config::Config) -> Location {
    (config.locale(), config.latitude(), config.longitude())
}

fn semantic_members(
    state: &Arc<WallState>,
    config: &skwd_wall_core::config::Config,
    query: &str,
    resolve: bool,
) -> Option<Result<Vec<String>, String>> {
    let search = if resolve {
        Some(crate::infrastructure::semantic_index::weather_search(
            state,
            query,
            config.weather_semantic_max_results(),
            config.weather_semantic_score_window(),
            config.weather_semantic_prominence(),
        ))
    } else {
        crate::infrastructure::semantic_index::weather_cached(
            state,
            query,
            config.weather_semantic_max_results(),
            config.weather_semantic_score_window(),
            config.weather_semantic_prominence(),
        )
    }?;
    Some(search.map(|keys| {
        visible_members(state, &keys)
            .into_iter()
            .take(config.weather_semantic_max_results())
            .collect()
    }))
}

pub(super) fn visible_members(state: &Arc<WallState>, keys: &[String]) -> Vec<String> {
    let items = state.with_db(|conn| db::list_wallpapers(conn, false)).unwrap_or_default();
    let visible = super::profile::Visibility::load(state);
    let allowed: BTreeSet<_> = items
        .iter()
        .filter(|item| visible.shows_item(item))
        .filter_map(|item| {
            item.get(wall_proto::rpc_params::KEY).and_then(serde_json::Value::as_str)
        })
        .collect();
    let mut seen = BTreeSet::new();
    keys.iter()
        .filter(|key| allowed.contains(key.as_str()) && seen.insert(key.as_str()))
        .cloned()
        .collect()
}

fn approved_members(
    state: &Arc<WallState>,
    config: &skwd_wall_core::config::Config,
    category: &str,
) -> Vec<String> {
    visible_members(state, &config.weather_approved(category))
}

fn approval_signature(
    config: &skwd_wall_core::config::Config,
    status: &WeatherStatus,
) -> Vec<String> {
    if status.source == WeatherSource::Semantic
        && status.selection == WeatherSelection::Approved
        && let Some(category) = status.category.as_deref()
    {
        config.weather_approved(category)
    } else {
        Vec::new()
    }
}

fn pause_query(
    source: WeatherSource,
    selection: WeatherSelection,
    query: Option<String>,
) -> Option<String> {
    if source == WeatherSource::Semantic && selection == WeatherSelection::Approved {
        None
    } else {
        query
    }
}

pub(crate) fn candidates(state: &Arc<WallState>, category: &str) -> WeatherCandidates {
    state.reload_config();
    let config = state.config().clone();
    let query = config.weather_semantic_query(category);
    let mut result = WeatherCandidates {
        category: category.to_owned(),
        query: query.clone(),
        ..WeatherCandidates::default()
    };
    let Some(query) = query else { return result };
    match crate::infrastructure::semantic_index::weather_preview_search(
        state,
        &query,
        config.weather_semantic_max_results(),
        config.weather_semantic_score_window(),
        config.weather_semantic_prominence(),
    ) {
        Ok(keys) => {
            result.member_keys = visible_members(state, &keys)
                .into_iter()
                .take(config.weather_semantic_max_results())
                .collect();
        }
        Err(error) => result.error = Some(error),
    }
    result
}

pub(crate) fn note_apply(
    state: &Arc<WallState>,
    source: ApplySource,
    outputs: &[String],
    key: Option<&str>,
) {
    if source.manual_choice() {
        super::outdoors::cancel_restore();
    }
    if super::outdoors::active(&state.config()) {
        super::outdoors::note_apply(state, source, outputs, key);
        return;
    }
    if !matches!(
        source,
        ApplySource::User
            | ApplySource::UserOverride
            | ApplySource::Random
            | ApplySource::Replay
            | ApplySource::Schedule
            | ApplySource::Workspace
    ) {
        return;
    }
    state.reload_config();
    let config = state.config().clone();
    if !config.weather_enabled() {
        return;
    }
    let mut guard = lock(&RUNTIME);
    let Some(runtime) = guard.as_mut() else { return };
    if let Some((category, playlist_id, query)) = selection(&config, &runtime.status.weather) {
        let query = pause_query(config.weather_source(), config.weather_selection(), query);
        if runtime.policy.category.as_deref() != Some(category)
            || runtime.policy.playlist_id != playlist_id
            || runtime.policy.source != config.weather_source()
            || runtime.policy.query != query
        {
            runtime.policy.paused.clear();
        }
        runtime.policy.category = Some(category.to_owned());
        runtime.policy.playlist_id = playlist_id;
        runtime.policy.source = config.weather_source();
        runtime.policy.query = query;
    }
    runtime.policy.paused.extend(outputs.iter().cloned());
    runtime.revision = runtime.revision.wrapping_add(1);
    runtime.save();
    drop(guard);
    super::playlist::reload();
}

#[cfg(test)]
fn status(state: &Arc<WallState>, current: Forecast) -> WeatherStatus {
    status_for(state, current, true)
}

#[cfg(test)]
fn status_for(state: &Arc<WallState>, current: Forecast, resolve_semantic: bool) -> WeatherStatus {
    resolved_for(state, current, resolve_semantic).status
}

fn resolved_for(state: &Arc<WallState>, current: Forecast, resolve_semantic: bool) -> Resolved {
    status_with(state, current, resolve_semantic, semantic_members)
}

fn pending_for(state: &Arc<WallState>) -> Resolved {
    let mut waiting = resolved_for(state, Forecast::default(), false);
    if waiting.status.status != WeatherState::NoLocation {
        let config = state.config();
        waiting.status.status =
            if config.weather_enabled() { WeatherState::Loading } else { WeatherState::Disabled };
    }
    waiting
}

fn status_with(
    state: &Arc<WallState>,
    current: Forecast,
    resolve_semantic: bool,
    search: impl FnOnce(
        &Arc<WallState>,
        &skwd_wall_core::config::Config,
        &str,
        bool,
    ) -> Option<Result<Vec<String>, String>>,
) -> Resolved {
    let config = state.config().clone();
    let mut status = WeatherStatus {
        enabled: config.weather_enabled(),
        source: config.weather_source(),
        selection: config.weather_selection(),
        weather: current.weather,
        fetched_at: current.fetched_at,
        ..WeatherStatus::default()
    };
    if let Some((category, id, query)) = selection(&config, &status.weather) {
        status.category = Some(category.to_owned());
        status.playlist_id = id;
        status.query = query;
    }
    if let Some(id) = status.playlist_id {
        status.playlist_name = state
            .with_db(db::playlists_all)
            .unwrap_or_default()
            .into_iter()
            .find(|row| row.id == id)
            .map(|row| row.name);
        status.member_keys = super::playlist::resolve_member_keys(state, id);
    }
    let mut search_unavailable = false;
    let mut search_pending = false;
    if config.weather_enabled() {
        if status.source == WeatherSource::Semantic
            && status.selection == WeatherSelection::Approved
            && let Some(category) = status.category.as_deref()
        {
            status.member_keys = approved_members(state, &config, category);
        } else if let Some(query) = &status.query {
            match search(state, &config, query, resolve_semantic) {
                Some(Ok(keys)) => status.member_keys = keys,
                Some(Err(error)) => {
                    log::warn!("weather: semantic search unavailable: {error}");
                    search_unavailable = true;
                }
                None => search_pending = true,
            }
        }
    }
    if resolve_semantic {
        state.reload_config();
        let latest = state.config().clone();
        if latest != config {
            reload();
            if location(&latest) != location(&config) {
                return pending_for(state);
            }
            return resolved_for(
                state,
                Forecast {
                    weather: status.weather,
                    fetched_at: status.fetched_at,
                    ..Forecast::default()
                },
                false,
            );
        }
    }
    status.resolved_count = status.member_keys.len();
    status.paused_outputs = state
        .with_db(db::playlist_assigns)
        .unwrap_or_default()
        .into_iter()
        .map(|(output, _)| output)
        .collect();
    if status.category.is_none() {
        status.category = weather_category::PRIORITY
            .iter()
            .find(|category| status.weather.iter().any(|tag| tag == **category))
            .map(|category| (*category).to_owned());
    }
    status.status = classify(&config, &status, search_pending, search_unavailable);
    Resolved { status, location: location(&config), config }
}

fn classify(
    config: &skwd_wall_core::config::Config,
    status: &WeatherStatus,
    search_pending: bool,
    search_unavailable: bool,
) -> WeatherState {
    if config.locale().trim().is_empty() && config.latitude() == 0.0 && config.longitude() == 0.0 {
        WeatherState::NoLocation
    } else if status.weather.is_empty() {
        WeatherState::Unavailable
    } else if status.playlist_id.is_none()
        && status.query.is_none()
        && !(status.source == WeatherSource::Semantic
            && status.selection == WeatherSelection::Approved)
    {
        WeatherState::Unmapped
    } else if search_pending {
        WeatherState::Loading
    } else if search_unavailable {
        WeatherState::SearchUnavailable
    } else if status.playlist_id.is_some() && status.playlist_name.is_none() {
        WeatherState::Missing
    } else if status.source == WeatherSource::Semantic && !status.enabled {
        WeatherState::Disabled
    } else if status.member_keys.is_empty() {
        WeatherState::Empty
    } else if !status.enabled {
        WeatherState::Disabled
    } else {
        WeatherState::Active
    }
}

pub(crate) fn snapshot(ctx: &Ctx) -> WeatherStatus {
    ctx.state.reload_config();
    if ctx.state.config().weather_source() == WeatherSource::Outdoors {
        return super::outdoors::snapshot(ctx);
    }
    super::outdoors::deactivate(&ctx.state.config());
    let config = ctx.state.config().clone();
    let Ok(_publish) = PUBLISH_LOCK.try_lock() else {
        reload();
        let guard = lock(&RUNTIME);
        let mut status = guard.as_ref().map_or_else(
            || WeatherStatus {
                enabled: config.weather_enabled(),
                source: config.weather_source(),
                selection: config.weather_selection(),
                status: WeatherState::Loading,
                ..WeatherStatus::default()
            },
            |runtime| runtime.status.clone(),
        );
        let location_matches = guard
            .as_ref()
            .is_some_and(|runtime| runtime.location.as_ref() == Some(&location(&config)));
        if location_matches
            && status.source == config.weather_source()
            && status.selection == config.weather_selection()
            && guard
                .as_ref()
                .is_some_and(|runtime| runtime.approved == approval_signature(&config, &status))
            && status.enabled == config.weather_enabled()
            && (config.weather_enabled())
            && selection(&config, &status.weather).is_some_and(|(category, id, query)| {
                status.category.as_deref() == Some(category)
                    && status.playlist_id == id
                    && status.query == query
                    && (status.selection != WeatherSelection::Approved
                        || status.member_keys == approved_members(&ctx.state, &config, category))
            })
        {
            return status;
        }
        if !location_matches {
            status.weather.clear();
            status.fetched_at = None;
        }
        status.enabled = config.weather_enabled();
        status.source = config.weather_source();
        status.selection = config.weather_selection();
        status.playlist_id = None;
        status.playlist_name = None;
        status.member_keys.clear();
        status.resolved_count = 0;
        status.query = None;
        status.category = None;
        if let Some((category, id, query)) = selection(&config, &status.weather) {
            status.category = Some(category.to_owned());
            status.playlist_id = id;
            status.query = query;
        }
        status.status =
            if config.weather_enabled() { WeatherState::Loading } else { WeatherState::Disabled };
        return status;
    };
    let current = cached(&config.locale(), config.latitude(), config.longitude());
    let status = publish_for(ctx, current, false, false);
    if config.weather_enabled() {
        reload();
    }
    status
}

pub(crate) fn refresh(ctx: &Ctx, force: bool, resume: bool) -> WeatherStatus {
    ctx.state.reload_config();
    if ctx.state.config().weather_source() == WeatherSource::Outdoors {
        return super::outdoors::refresh(ctx, force);
    }
    super::outdoors::deactivate(&ctx.state.config());
    let _refresh = lock(&REFRESH_LOCK);
    if force {
        crate::infrastructure::semantic_index::clear_failed_weather_search();
    }
    loop {
        ctx.state.reload_config();
        let config = ctx.state.config().clone();
        let current = forecast(&config.locale(), config.latitude(), config.longitude(), force);
        let _publish = lock(&PUBLISH_LOCK);
        ctx.state.reload_config();
        let latest = ctx.state.config().clone();
        if latest.locale() == config.locale()
            && latest.latitude() == config.latitude()
            && latest.longitude() == config.longitude()
        {
            return publish(ctx, Some(current), resume);
        }
    }
}

fn publish(ctx: &Ctx, current: Option<Forecast>, resume: bool) -> WeatherStatus {
    publish_for(ctx, current, resume, true)
}

fn publish_for(
    ctx: &Ctx,
    current: Option<Forecast>,
    resume: bool,
    resolve_semantic: bool,
) -> WeatherStatus {
    let pending = current.is_none();
    let resolved = resolved_for(&ctx.state, current.unwrap_or_default(), resolve_semantic);
    publish_resolved(ctx, resolved, pending, resume)
}

fn publish_resolved(
    ctx: &Ctx,
    mut resolved: Resolved,
    pending: bool,
    resume: bool,
) -> WeatherStatus {
    ctx.state.reload_config();
    let latest = ctx.state.config().clone();
    if latest != resolved.config {
        reload();
        resolved = if location(&latest) == resolved.location {
            resolved_for(
                &ctx.state,
                Forecast {
                    weather: resolved.status.weather,
                    fetched_at: resolved.status.fetched_at,
                    ..Forecast::default()
                },
                false,
            )
        } else {
            pending_for(&ctx.state)
        };
    }
    let mut status = resolved.status;
    let approved = approval_signature(&resolved.config, &status);
    if pending && status.status != WeatherState::NoLocation {
        let config = ctx.state.config();
        status.status =
            if config.weather_enabled() { WeatherState::Loading } else { WeatherState::Disabled };
    }
    let path = PathBuf::from(ctx.state.config().cache_dir()).join(PAUSE_FILE);
    let mut guard = lock(&RUNTIME);
    if guard.as_ref().is_none_or(|runtime| runtime.path != path) {
        *guard = Some(Runtime::load(path));
    }
    let runtime = guard.as_mut().expect("weather runtime initialized");
    let changed_approval = runtime.approved != approved;
    if changed_approval {
        runtime.approved = approved;
        runtime.revision = runtime.revision.wrapping_add(1);
    }
    let moved = runtime.location.as_ref() != Some(&resolved.location);
    runtime.location = Some(resolved.location);
    if moved {
        runtime.revision = runtime.revision.wrapping_add(1);
    }
    if resume {
        runtime.policy.paused.clear();
        runtime.save();
    }
    let changed = runtime.update(status) || moved || changed_approval;
    let status = runtime.status.clone();
    drop(guard);
    if changed {
        ctx.events.publish(
            ev::WEATHER_CHANGED,
            serde_json::to_value(&status).expect("weather status serialization"),
        );
        super::playlist::reload();
        super::rotation::wake();
    }
    status
}

pub(crate) fn start(ctx: Ctx) {
    let config = ctx.state.config().clone();
    let mut runtime = Runtime::load(PathBuf::from(config.cache_dir()).join(PAUSE_FILE));
    runtime.status.enabled = config.weather_enabled();
    runtime.status.source = config.weather_source();
    runtime.status.selection = config.weather_selection();
    if config.weather_enabled() {
        runtime.status.status = WeatherState::Loading;
    }
    *lock(&RUNTIME) = Some(runtime);
    tokio::spawn(async move {
        loop {
            let task = ctx.clone();
            let enabled = tokio::task::spawn_blocking(move || {
                task.state.reload_config();
                let enabled = {
                    let config = task.state.config();
                    config.weather_enabled()
                };
                if enabled || lock(&RUNTIME).as_ref().is_some_and(|runtime| runtime.status.enabled)
                {
                    refresh(&task, false, false);
                }
                enabled
            })
            .await
            .unwrap_or(false);
            if enabled {
                let wait = if super::outdoors::active(&ctx.state.config()) {
                    super::outdoors::wait().min(refresh_interval())
                } else {
                    refresh_interval()
                };
                crate::infrastructure::wake::wake_or_timeout(&WAKE, wait).await;
            } else {
                WAKE.notified().await;
            }
        }
    });
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) fn test_status(state: &Arc<WallState>, status: Option<WeatherStatus>) {
    state.reload_config();
    let location = location(&state.config());
    *lock(&RUNTIME) = status.map(|status| Runtime {
        approved: approval_signature(&state.config(), &status),
        status,
        policy: Policy::default(),
        revision: 1,
        path: std::env::temp_dir().join(PAUSE_FILE),
        location: Some(location),
    });
}
