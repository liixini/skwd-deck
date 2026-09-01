use std::path::PathBuf;

use serde_json::{Value, json};
use skwd_wall_core::WallState;
use skwd_wall_core::domain::wallpaper::{LastApplied, OutputPolicy, OutputTargetState, Wallpaper};
use skwd_wall_core::outputs::OutputInfo;

fn monitor_store_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(crate) fn last_applied_path() -> PathBuf {
    skwd_wall_core::paths::cache_dir().join("last-applied.json")
}

pub(crate) fn monitors_path() -> PathBuf {
    skwd_wall_core::paths::cache_dir().join("monitors.json")
}

fn wallpaper_from(value: &Value) -> Option<Wallpaper> {
    let kind = value.get("type").and_then(Value::as_str).unwrap_or(wall_proto::kind::STATIC);
    let wallpaper = Wallpaper {
        kind: kind.to_string(),
        path: value.get("path").and_then(Value::as_str).unwrap_or_default().to_string(),
        we_id: value.get("we_id").and_then(Value::as_str).unwrap_or_default().to_string(),
    };
    (!wallpaper.is_empty()).then_some(wallpaper)
}

fn wallpaper_json(wallpaper: &Wallpaper) -> Value {
    json!({ "type": wallpaper.kind, "path": wallpaper.path, "we_id": wallpaper.we_id })
}

fn remember_wallpaper(entry: &mut Value, wallpaper: &Wallpaper, overwrite: bool) {
    if overwrite || entry.get("wallpaper").and_then(wallpaper_from).is_none() {
        entry["wallpaper"] = wallpaper_json(wallpaper);
    }
}

pub(crate) fn read_last_applied() -> LastApplied {
    let Ok(text) = std::fs::read_to_string(last_applied_path()) else {
        return legacy_last_applied();
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return legacy_last_applied();
    };
    let slot = |key: &str| value.get(key).and_then(wallpaper_from);
    let recorded =
        LastApplied { any: slot("any"), landscape: slot("landscape"), portrait: slot("portrait") };
    if recorded.any.is_none() { legacy_last_applied() } else { recorded }
}

fn legacy_last_applied() -> LastApplied {
    let Ok(text) = std::fs::read_to_string(super::persistence::last_wallpaper_path()) else {
        return LastApplied::default();
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return LastApplied::default();
    };
    LastApplied { any: wallpaper_from(&value), landscape: None, portrait: None }
}

pub(crate) fn record_last_applied(wallpaper: &Wallpaper, portrait: Option<bool>) {
    if wallpaper.is_empty() {
        return;
    }
    let mut last = read_last_applied();
    match portrait {
        Some(portrait) => last.record(wallpaper, portrait),
        None => last.any = Some(wallpaper.clone()),
    }
    let payload = json!({
        "any": last.any.as_ref().map(wallpaper_json),
        "landscape": last.landscape.as_ref().map(wallpaper_json),
        "portrait": last.portrait.as_ref().map(wallpaper_json),
    });
    let _ =
        skwd_wall_core::paths::atomic_write(&last_applied_path(), payload.to_string().as_bytes());
}

pub(crate) fn record_apply(output: &str, kind: &str, path: &str, we_id: &str) {
    let wallpaper =
        Wallpaper { kind: kind.to_string(), path: path.to_string(), we_id: we_id.to_string() };
    if wallpaper.is_empty() {
        return;
    }
    let outputs = skwd_wall_core::outputs::enumerate();
    if output == "*" {
        record_last_applied(&wallpaper, None);
        remember_output_wallpapers(&outputs, &wallpaper, true);
        return;
    }
    let names: Vec<&str> = output.split(',').map(str::trim).collect();
    let touched: Vec<&OutputInfo> =
        outputs.iter().filter(|info| names.contains(&info.name.as_str())).collect();
    if touched.is_empty() {
        record_last_applied(&wallpaper, None);
        return;
    }
    for info in &touched {
        record_last_applied(&wallpaper, Some(info.portrait()));
    }
    remember_output_wallpapers(&touched.into_iter().cloned().collect::<Vec<_>>(), &wallpaper, true);
}

pub(crate) fn record_restored(output: &str, kind: &str, path: &str, we_id: &str) {
    let wallpaper =
        Wallpaper { kind: kind.to_string(), path: path.to_string(), we_id: we_id.to_string() };
    if wallpaper.is_empty() {
        return;
    }
    let outputs = skwd_wall_core::outputs::enumerate();
    let names: Vec<&str> = output.split(',').map(str::trim).collect();
    let touched: Vec<OutputInfo> = outputs
        .into_iter()
        .filter(|info| output == "*" || names.contains(&info.name.as_str()))
        .collect();
    remember_output_wallpapers(&touched, &wallpaper, false);
}

fn remember_output_wallpapers(outputs: &[OutputInfo], wallpaper: &Wallpaper, overwrite: bool) {
    let _guard = monitor_store_lock();
    let mut known = read_monitors();
    for output in outputs {
        let entry = known.entry(output.identity_key()).or_insert_with(|| json!({}));
        remember_wallpaper(entry, wallpaper, overwrite);
        entry["connector"] = json!(output.name);
        entry["make"] = json!(output.make);
        entry["model"] = json!(output.model);
    }
    write_monitors(&known);
}

fn read_monitors() -> serde_json::Map<String, Value> {
    std::fs::read_to_string(monitors_path())
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

fn write_monitors(known: &serde_json::Map<String, Value>) {
    let _ = skwd_wall_core::paths::atomic_write(
        &monitors_path(),
        Value::Object(known.clone()).to_string().as_bytes(),
    );
}

fn identity_wallpaper(output: &OutputInfo) -> Option<Wallpaper> {
    let _guard = monitor_store_lock();
    let known = read_monitors();
    if let Some(exact) = known.get(&output.identity_key()).and_then(|entry| entry.get("wallpaper"))
    {
        return wallpaper_from(exact);
    }
    if output.identity() == output.name {
        return None;
    }
    let mut moved = known.values().filter(|entry| {
        entry.get("make").and_then(Value::as_str) == Some(output.make.as_str())
            && entry.get("model").and_then(Value::as_str) == Some(output.model.as_str())
            && entry.get("wallpaper").is_some()
    });
    let candidate = moved.next()?;
    if moved.next().is_some() {
        return None;
    }
    log::info!("restore: {} matched a known monitor on a different connector", output.name);
    candidate.get("wallpaper").and_then(wallpaper_from)
}

pub(crate) fn wallpaper_present(wallpaper: &Wallpaper) -> bool {
    wallpaper_is_present(wallpaper)
}

fn wallpaper_is_present(wallpaper: &Wallpaper) -> bool {
    if wallpaper.kind == wall_proto::kind::WE {
        return !wallpaper.we_id.is_empty();
    }
    !wallpaper.path.is_empty() && std::path::Path::new(&wallpaper.path).exists()
}

pub(crate) fn remember_monitors(outputs: &[OutputInfo]) {
    let _guard = monitor_store_lock();
    let mut known = read_monitors();
    for output in outputs {
        let entry = known.entry(output.identity_key()).or_insert_with(|| json!({}));
        entry["connector"] = json!(output.name);
        entry["make"] = json!(output.make);
        entry["model"] = json!(output.model);
        entry["portrait"] = json!(output.portrait());
        entry["width"] = json!(output.width);
        entry["height"] = json!(output.height);
    }
    write_monitors(&known);
}

fn configured_policy(state: &WallState, output: &OutputInfo) -> Option<OutputPolicy> {
    let config = state.config();
    let policies = config.display().output_policies();
    let entry = policies.get(&output.identity()).or_else(|| policies.get(&output.name)).cloned()?;
    drop(config);
    let mode = entry.get("mode").and_then(Value::as_str)?;
    OutputPolicy::parse(mode, wallpaper_from(&entry))
}

pub(crate) fn targets(state: &WallState, outputs: &[OutputInfo]) -> Vec<OutputTargetState> {
    let cache = state.config().cache_dir();
    let recorded = skwd_wall_core::audio::read_state(&cache);
    let live = state.renderers().assignments();
    outputs
        .iter()
        .filter_map(|output| {
            let pinned = identity_wallpaper(output)
                .or_else(|| recorded.get(&output.name).and_then(wallpaper_from))
                .filter(wallpaper_is_present);
            let policy = configured_policy(state, output).unwrap_or_else(|| {
                pinned.clone().map_or(OutputPolicy::FollowDimension, OutputPolicy::Pin)
            });
            if pinned.is_none() && (output.width <= 0 || output.height <= 0) {
                log::info!(
                    "restore: {} has no geometry yet, deferring its first-time wallpaper",
                    output.name
                );
                return None;
            }
            Some(OutputTargetState {
                output: output.name.clone(),
                portrait: output.portrait(),
                policy,
                live: live.get(&output.name).cloned(),
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "restore_policy_tests.rs"]
mod tests;

pub(crate) fn forget_wallpaper(cache: &str, path: &str, we_id: &str) {
    let matches = |wallpaper: &Wallpaper| {
        (!path.is_empty() && wallpaper.path == path)
            || (!we_id.is_empty() && wallpaper.we_id == we_id)
    };
    let last = read_last_applied();
    let keep = |slot: Option<Wallpaper>| slot.filter(|wallpaper| !matches(wallpaper));
    let payload = json!({
        "any": keep(last.any).as_ref().map(wallpaper_json),
        "landscape": keep(last.landscape).as_ref().map(wallpaper_json),
        "portrait": keep(last.portrait).as_ref().map(wallpaper_json),
    });
    let _ =
        skwd_wall_core::paths::atomic_write(&last_applied_path(), payload.to_string().as_bytes());

    {
        let _guard = monitor_store_lock();
        let mut known = read_monitors();
        let mut touched = false;
        for entry in known.values_mut() {
            if entry
                .get("wallpaper")
                .and_then(wallpaper_from)
                .is_some_and(|wallpaper| matches(&wallpaper))
            {
                entry.as_object_mut().map(|entry| entry.remove("wallpaper"));
                touched = true;
            }
        }
        if touched {
            write_monitors(&known);
        }
    }

    let state = skwd_wall_core::audio::read_state(cache);
    let Some(outputs) = state.as_object() else {
        return;
    };
    let kept: serde_json::Map<String, Value> = outputs
        .iter()
        .filter(|(_, entry)| !wallpaper_from(entry).is_some_and(|wallpaper| matches(&wallpaper)))
        .map(|(name, entry)| (name.clone(), entry.clone()))
        .collect();
    if kept.len() != outputs.len() {
        log::info!(
            "forgot {} pinned wallpaper record(s) for a deleted item",
            outputs.len() - kept.len()
        );
        skwd_wall_core::audio::write_state(cache, &Value::Object(kept));
    }
}

pub(crate) fn purge_missing(cache: &str) {
    let last = read_last_applied();
    let keep = |slot: Option<Wallpaper>| slot.filter(wallpaper_is_present);
    let (any, landscape, portrait) =
        (keep(last.any.clone()), keep(last.landscape.clone()), keep(last.portrait.clone()));
    if (any.is_none() && last.any.is_some())
        || (landscape.is_none() && last.landscape.is_some())
        || (portrait.is_none() && last.portrait.is_some())
    {
        log::info!("restore: dropped last-applied slot(s) whose wallpaper no longer exists");
        let payload = json!({
            "any": any.as_ref().map(wallpaper_json),
            "landscape": landscape.as_ref().map(wallpaper_json),
            "portrait": portrait.as_ref().map(wallpaper_json),
        });
        let _ = skwd_wall_core::paths::atomic_write(
            &last_applied_path(),
            payload.to_string().as_bytes(),
        );
    }
    let state = skwd_wall_core::audio::read_state(cache);
    let Some(outputs) = state.as_object() else {
        return;
    };
    let kept: serde_json::Map<String, Value> = outputs
        .iter()
        .filter(|(_, entry)| {
            wallpaper_from(entry).is_none_or(|wallpaper| wallpaper_is_present(&wallpaper))
        })
        .map(|(name, entry)| (name.clone(), entry.clone()))
        .collect();
    if kept.len() != outputs.len() {
        log::info!(
            "restore: dropped {} output record(s) whose wallpaper no longer exists",
            outputs.len() - kept.len()
        );
        skwd_wall_core::audio::write_state(cache, &Value::Object(kept));
    }
    {
        let _guard = monitor_store_lock();
        let mut known = read_monitors();
        let mut touched = false;
        for entry in known.values_mut() {
            let stale = entry
                .get("wallpaper")
                .and_then(wallpaper_from)
                .is_some_and(|wallpaper| !wallpaper_is_present(&wallpaper));
            if stale {
                entry.as_object_mut().map(|entry| entry.remove("wallpaper"));
                touched = true;
            }
        }
        if touched {
            write_monitors(&known);
        }
    }
}

pub(crate) fn known_monitors(live: &[OutputInfo]) -> Value {
    let _guard = monitor_store_lock();
    let connected: std::collections::HashSet<String> =
        live.iter().map(OutputInfo::identity_key).collect();
    let rows: Vec<Value> = read_monitors()
        .into_iter()
        .map(|(key, entry)| {
            json!({
                "id": key,
                "connector": entry.get("connector").cloned().unwrap_or(Value::Null),
                "make": entry.get("make").cloned().unwrap_or(Value::Null),
                "model": entry.get("model").cloned().unwrap_or(Value::Null),
                "portrait": entry.get("portrait").cloned().unwrap_or(Value::Null),
                "width": entry.get("width").cloned().unwrap_or(Value::Null),
                "height": entry.get("height").cloned().unwrap_or(Value::Null),
                "wallpaper": entry.get("wallpaper").cloned().unwrap_or(Value::Null),
                "connected": connected.contains(&key),
            })
        })
        .collect();
    json!({ "monitors": rows })
}

pub(crate) fn assign_remembered_monitor(
    id: &str,
    kind: &str,
    path: &str,
    we_id: &str,
) -> Option<String> {
    let wallpaper =
        Wallpaper { kind: kind.to_string(), path: path.to_string(), we_id: we_id.to_string() };
    if wallpaper.is_empty() {
        return None;
    }
    let (connector, portrait) = {
        let _guard = monitor_store_lock();
        let mut known = read_monitors();
        let entry = known.get_mut(id)?;
        entry["wallpaper"] = wallpaper_json(&wallpaper);
        let connector =
            entry.get("connector").and_then(Value::as_str).unwrap_or_default().to_string();
        let portrait = entry.get("portrait").and_then(Value::as_bool);
        write_monitors(&known);
        (connector, portrait)
    };
    record_last_applied(&wallpaper, portrait);
    Some(connector)
}

pub(crate) fn remembered_monitor_connector(id: &str) -> Option<String> {
    let _guard = monitor_store_lock();
    read_monitors()
        .get(id)
        .and_then(|entry| entry.get("connector"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

pub(crate) fn forget_monitor(cache: &str, id: &str) -> bool {
    let entry = {
        let _guard = monitor_store_lock();
        let mut known = read_monitors();
        let Some(entry) = known.remove(id) else {
            return false;
        };
        write_monitors(&known);
        entry
    };
    let connector = entry.get("connector").and_then(Value::as_str).unwrap_or_default();
    if connector.is_empty() {
        return true;
    }
    let state = skwd_wall_core::audio::read_state(cache);
    if let Some(outputs) = state.as_object()
        && outputs.contains_key(connector)
    {
        let kept: serde_json::Map<String, Value> = outputs
            .iter()
            .filter(|(name, _)| name.as_str() != connector)
            .map(|(name, entry)| (name.clone(), entry.clone()))
            .collect();
        skwd_wall_core::audio::write_state(cache, &Value::Object(kept));
    }
    log::info!("forgot monitor {id}");
    true
}
