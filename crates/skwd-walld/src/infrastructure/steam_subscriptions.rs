use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use skwd_wall_core::{WallState, we::SteamTracking};

use crate::backend::events::EventPublisher;
use crate::infrastructure::steam_download::{self, SteamInventory};

static TRACKING_GATE: Mutex<()> = Mutex::new(());
const PROJECT_FILE: &str = "project.json";
const INSTALLED_SECTION: &str = "\"WorkshopItemsInstalled\"";
const SECTION_OPEN: &str = "{";
const SECTION_CLOSE: &str = "}";
const DEPENDENT_SEPARATOR: &str = ", ";

fn manifest_ids(path: &Path) -> HashSet<String> {
    let Ok(contents) = std::fs::read_to_string(path) else { return HashSet::new() };
    let mut installed = HashSet::new();
    let mut section = false;
    let mut depth = 0usize;
    for line in contents.lines().map(str::trim) {
        if !section {
            section = line == INSTALLED_SECTION;
            continue;
        }
        if line == SECTION_OPEN {
            depth += 1;
        } else if line == SECTION_CLOSE {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                break;
            }
        } else if depth == 1
            && let Some(id) = line.strip_prefix('"').and_then(|line| line.strip_suffix('"'))
        {
            installed.insert(id.to_string());
        }
    }
    installed
}

pub(crate) fn manifest_paths(config: &skwd_wall_core::config::Config) -> Vec<PathBuf> {
    let mut roots = vec![config.steam_install_root()];
    let source = config.we_dir();
    let tracking = SteamTracking::load();
    let mut paths = item_paths(&source).into_values().collect::<Vec<_>>();
    if tracking.source == source {
        paths.extend(tracking.seen.into_values());
        paths.extend(tracking.hidden.into_values());
        paths.extend(tracking.owned.into_values().flat_map(HashMap::into_values));
    }
    roots.extend(
        paths.into_iter().filter_map(|item| {
            item.parent().and_then(skwd_wall_core::paths::steam_root_for_content)
        }),
    );
    roots.sort();
    roots.dedup();
    roots
        .into_iter()
        .map(|root| root.join(skwd_wall_core::paths::STEAM_WORKSHOP_MANIFEST))
        .collect()
}

fn item_paths(source: &Path) -> HashMap<String, PathBuf> {
    std::fs::read_dir(source)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let id = entry.file_name().to_string_lossy().into_owned();
            if !id.chars().all(|ch| ch.is_ascii_digit()) {
                return None;
            }
            entry
                .path()
                .join(PROJECT_FILE)
                .is_file()
                .then(|| entry.path().canonicalize().ok().map(|path| (id, path)))
                .flatten()
        })
        .collect()
}

fn include_missing(
    source: &Path,
    items: &mut HashMap<String, PathBuf>,
    tracked: &HashMap<String, PathBuf>,
) {
    for (id, path) in tracked {
        let replaced = source.join(id).canonicalize().is_ok_and(|actual| actual != *path);
        if !items.contains_key(id) && !replaced {
            items.insert(id.clone(), path.clone());
        }
    }
}

fn matches_path(folder: &Path, item: &Path) -> bool {
    folder.canonicalize().is_ok_and(|path| path == item)
}

fn switch_account(tracking: &mut SteamTracking, source: &Path, account: &str) {
    if tracking.account == account && tracking.source == source {
        return;
    }
    let mut owned = if tracking.source == source {
        std::mem::take(&mut tracking.owned)
    } else {
        HashMap::new()
    };
    let active = if tracking.source == source {
        std::mem::take(&mut tracking.active)
    } else {
        HashMap::new()
    };
    if tracking.source == source && !tracking.account.is_empty() {
        owned.entry(tracking.account.clone()).or_default().extend(
            tracking
                .seen
                .iter()
                .chain(&tracking.hidden)
                .map(|(id, path)| (id.clone(), path.clone())),
        );
    }
    *tracking = SteamTracking {
        account: account.to_string(),
        source: source.to_path_buf(),
        owned,
        active,
        ..SteamTracking::default()
    };
}

pub(crate) fn hidden(state: &Arc<WallState>, id: &str, item: &Path) -> bool {
    SteamTracking::load().hides(&state.config().we_dir(), id, item)
}

pub(crate) fn subscribed(state: &Arc<WallState>, id: &str, item: &Path) -> bool {
    SteamTracking::load().subscribed(&state.config().we_dir(), id, item)
}

pub(crate) struct SteamConfirmation {
    source: PathBuf,
    install_root: PathBuf,
    tracking: SteamTracking,
    installed: HashMap<PathBuf, HashSet<String>>,
}

impl SteamConfirmation {
    pub(crate) fn new(config: &skwd_wall_core::config::Config) -> Self {
        Self {
            source: config.we_dir(),
            install_root: config.steam_install_root(),
            tracking: SteamTracking::load(),
            installed: HashMap::new(),
        }
    }

    pub(crate) fn required(&mut self, id: &str, item: &Path) -> bool {
        let Ok(actual) = item.canonicalize() else { return false };
        if self.tracking.source == self.source
            && self.tracking.owned.values().any(|owned| owned.get(id) == Some(&actual))
        {
            return true;
        }
        let mut roots = vec![self.install_root.clone()];
        if actual.file_name().is_some_and(|name| name == id)
            && let Some(root) =
                actual.parent().and_then(skwd_wall_core::paths::steam_root_for_content)
        {
            roots.push(root);
        }
        roots.into_iter().any(|root| {
            let expected = root.join(skwd_wall_core::paths::STEAM_WORKSHOP_CONTENT).join(id);
            let manifest = root.join(skwd_wall_core::paths::STEAM_WORKSHOP_MANIFEST);
            expected.canonicalize().is_ok_and(|path| path == actual)
                && self
                    .installed
                    .entry(root)
                    .or_insert_with(|| manifest_ids(&manifest))
                    .contains(id)
        })
    }
}

pub(crate) fn requires_steam_confirmation(state: &Arc<WallState>, id: &str, item: &Path) -> bool {
    SteamConfirmation::new(&state.config()).required(id, item)
}

pub(crate) fn unsubscribe(
    state: &Arc<WallState>,
    publisher: &dyn EventPublisher,
    id: &str,
    warn_on_failure: bool,
) -> Result<(), String> {
    let _guard = TRACKING_GATE.lock().map_err(|error| error.to_string())?;
    let source = state.config().we_dir();
    let item = source.join(id).canonicalize().map_err(|error| error.to_string())?;
    let candidates = dependents::candidates(&source, &item);
    let mut ids = candidates
        .iter()
        .filter(|candidate| candidate.id.chars().all(|ch| ch.is_ascii_digit()))
        .map(|candidate| candidate.id.clone())
        .collect::<Vec<_>>();
    ids.push(id.to_string());
    ids.sort();
    ids.dedup();
    let inventory = steam_download::inventory(&ids)?;
    let mut tracking = SteamTracking::load();
    switch_account(&mut tracking, &source, &inventory.account);
    let native = inventory.installed.get(id).is_some_and(|folder| matches_path(folder, &item))
        || tracking.seen.get(id) == Some(&item)
        || tracking.owned.get(&inventory.account).and_then(|owned| owned.get(id)) == Some(&item);
    if !(native && inventory.subscribed.contains(id)) {
        return Err("This item is not a Steam Client subscription".into());
    }
    let blocking = dependents::blocking(&candidates, &item, &inventory, &tracking);
    if !blocking.is_empty() {
        return Err(format!(
            "Dependent presets still need this item: {}",
            blocking.join(DEPENDENT_SEPARATOR)
        ));
    }
    steam_download::run_unsubscribe(id, warn_on_failure, publisher)?;
    tracking.seen.remove(id);
    if let Some(active) = tracking.active.get_mut(&inventory.account) {
        active.remove(id);
    }
    tracking.owned.entry(inventory.account).or_default().insert(id.to_string(), item.clone());
    tracking.hidden.insert(id.to_string(), item);
    tracking.save().map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn reconcile(state: &Arc<WallState>, publisher: &dyn EventPublisher) {
    let Ok(_guard) = TRACKING_GATE.lock() else { return };
    let source = state.config().we_dir();
    let mut items = item_paths(&source);
    let mut tracking = SteamTracking::load();
    let known = tracking.source == source && !tracking.account.is_empty();
    if items.is_empty()
        && (!known
            || (tracking.seen.is_empty()
                && tracking.hidden.is_empty()
                && tracking.active.values().all(HashMap::is_empty)
                && tracking.owned.values().all(HashMap::is_empty)))
    {
        return;
    }
    let mut ids = items.keys().cloned().collect::<HashSet<_>>();
    if known {
        ids.extend(tracking.seen.keys().cloned());
        ids.extend(tracking.hidden.keys().cloned());
        ids.extend(tracking.active.values().flat_map(|active| active.keys().cloned()));
        ids.extend(tracking.owned.values().flat_map(|owned| owned.keys().cloned()));
    }
    let ids = ids.into_iter().collect::<Vec<_>>();
    let inventory = match steam_download::inventory(&ids) {
        Ok(inventory) => inventory,
        Err(error) => {
            log::info!("steam inventory unavailable: {error}");
            if known {
                include_missing(&source, &mut items, &tracking.hidden);
            }
            cleanup_hidden(state, publisher, &source, &items, &tracking, &HashSet::new());
            return;
        }
    };
    if known && tracking.account == inventory.account {
        include_missing(&source, &mut items, &tracking.seen);
        include_missing(&source, &mut items, &tracking.hidden);
    }
    if known && let Some(active) = tracking.active.get(&inventory.account) {
        include_missing(&source, &mut items, active);
    }
    if known && let Some(owned) = tracking.owned.get(&inventory.account) {
        include_missing(&source, &mut items, owned);
    }
    let removed = reconcile_inventory(&source, &items, &inventory, &mut tracking)
        .into_iter()
        .collect::<HashSet<_>>();
    if let Err(error) = tracking.save() {
        log::warn!("steam inventory could not be saved: {error}");
        return;
    }
    cleanup_hidden(state, publisher, &source, &items, &tracking, &removed);
}

fn cleanup_hidden(
    state: &Arc<WallState>,
    publisher: &dyn EventPublisher,
    source: &Path,
    items: &HashMap<String, PathBuf>,
    tracking: &SteamTracking,
    announced: &HashSet<String>,
) {
    if tracking.source != source {
        return;
    }
    for (id, item) in items {
        if tracking.hidden.get(id) == Some(item) {
            crate::infrastructure::removal::remove_we_record(
                state,
                publisher,
                id,
                Some(item),
                announced.contains(id),
            );
        }
    }
}

fn reconcile_inventory(
    source: &Path,
    items: &HashMap<String, PathBuf>,
    inventory: &SteamInventory,
    tracking: &mut SteamTracking,
) -> Vec<String> {
    switch_account(tracking, source, &inventory.account);
    let mut removed = Vec::new();
    for (id, item) in items {
        let installed =
            inventory.installed.get(id).is_some_and(|folder| matches_path(folder, item));
        let seen = tracking.seen.get(id) == Some(item);
        let current_account_owns =
            tracking.owned.get(&inventory.account).and_then(|owned| owned.get(id)) == Some(item);
        let other_account_active = tracking
            .active
            .iter()
            .any(|(account, active)| account != &inventory.account && active.get(id) == Some(item));
        if inventory.subscribed.contains(id) {
            if installed || current_account_owns {
                tracking.seen.insert(id.clone(), item.clone());
                tracking
                    .active
                    .entry(inventory.account.clone())
                    .or_default()
                    .insert(id.clone(), item.clone());
            }
            if installed {
                tracking
                    .owned
                    .entry(inventory.account.clone())
                    .or_default()
                    .insert(id.clone(), item.clone());
            }
            tracking.hidden.remove(id);
        } else {
            tracking.seen.remove(id);
            if let Some(active) = tracking.active.get_mut(&inventory.account) {
                active.remove(id);
            }
            if !other_account_active
                && (installed
                    || seen
                    || current_account_owns
                    || tracking.hidden.get(id) == Some(item))
            {
                let newly_hidden = tracking.hidden.get(id) != Some(item);
                tracking.hidden.insert(id.clone(), item.clone());
                if newly_hidden {
                    removed.push(id.clone());
                }
            }
        }
    }
    removed
}

mod dependents;

#[cfg(test)]
#[path = "steam_subscriptions/tests.rs"]
mod tests;
