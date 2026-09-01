use std::collections::HashMap;
use std::hash::BuildHasher;
use std::path::PathBuf;

use serde_json::{Map, Value};

pub fn state_path(cache_dir: &str) -> PathBuf {
    PathBuf::from(cache_dir).join("outputs.json")
}

pub fn read_state(cache_dir: &str) -> Value {
    std::fs::read_to_string(state_path(cache_dir))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| Value::Object(Map::new()))
}

pub fn write_state(cache_dir: &str, state: &Value) {
    let path = state_path(cache_dir);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let Ok(text) = serde_json::to_string(state) else {
        return;
    };
    let _ = crate::paths::atomic_write(&path, text.as_bytes());
}

pub fn entry(ty: &str, path: &str, we_id: &str, mute: bool, volume: u32) -> Value {
    serde_json::json!({
        "type": ty,
        "path": path,
        "we_id": we_id,
        "mute": mute,
        "volume": volume.min(100),
    })
}

pub fn record_outputs<S: BuildHasher>(
    cache_dir: &str,
    outputs: &[String],
    ty: &str,
    path: &str,
    we_id: &str,
    mute: &HashMap<String, bool, S>,
    volume: &HashMap<String, u32, S>,
    default_mute: bool,
    default_volume: u32,
) {
    let mut map = Map::new();
    let keys: Vec<String> =
        if outputs.is_empty() { vec!["*".to_string()] } else { outputs.to_vec() };
    for out in keys {
        let out_mute = mute.get(&out).copied().unwrap_or(default_mute);
        let out_vol = volume.get(&out).copied().unwrap_or(default_volume);
        map.insert(out, entry(ty, path, we_id, out_mute, out_vol));
    }
    write_state(cache_dir, &Value::Object(map));
}

pub fn expand_wildcard(cache_dir: &str, outputs: &[String]) {
    let mut st = read_state(cache_dir);
    let Some(map) = st.as_object_mut() else {
        return;
    };
    let Some(star) = map.remove("*") else {
        return;
    };
    for out in outputs {
        map.entry(out.clone()).or_insert_with(|| star.clone());
    }
    write_state(cache_dir, &st);
}

pub fn set_entry(
    cache_dir: &str,
    output: &str,
    ty: &str,
    path: &str,
    we_id: &str,
    mute: bool,
    volume: u32,
) {
    let mut st = read_state(cache_dir);
    let mut map = st.as_object().cloned().unwrap_or_default();
    map.insert(output.to_string(), entry(ty, path, we_id, mute, volume));
    st = Value::Object(map);
    write_state(cache_dir, &st);
}

pub fn carried_audio(prev: &Value, output: &str, def_mute: bool, def_vol: u32) -> (bool, u32) {
    let Some(map) = prev.as_object() else {
        return (def_mute, def_vol);
    };
    let pick = |ent: &Value| {
        (
            ent.get("mute").and_then(Value::as_bool).unwrap_or(def_mute),
            ent.get("volume").and_then(Value::as_u64).map_or(def_vol, |vol| (vol as u32).min(100)),
        )
    };
    if output != "*" {
        return map.get(output).or_else(|| map.get("*")).map_or((def_mute, def_vol), pick);
    }
    if let Some(ent) = map.get("*") {
        return pick(ent);
    }
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort();
    keys.iter()
        .map(|key| &map[key.as_str()])
        .find(|ent| !is_muted(ent))
        .or_else(|| keys.first().map(|key| &map[key.as_str()]))
        .map_or((def_mute, def_vol), pick)
}

pub fn resolve_defaults(
    cache_dir: &str,
    output: &str,
    def_mute: bool,
    def_vol: u32,
) -> (bool, u32) {
    carried_audio(&read_state(cache_dir), output, def_mute, def_vol)
}

pub fn update_audio(
    cache_dir: &str,
    filter: Option<&[String]>,
    mute: Option<bool>,
    volume: Option<u32>,
) {
    let mut state = read_state(cache_dir);
    let Some(map) = state.as_object_mut() else {
        return;
    };
    let keys: Vec<String> = map.keys().cloned().collect();
    for out in keys {
        if let Some(flt) = filter
            && !flt.iter().any(|name| name == &out)
        {
            continue;
        }
        let Some(ent) = map.get_mut(&out).and_then(Value::as_object_mut) else {
            continue;
        };
        if let Some(flag) = mute {
            ent.insert("mute".into(), Value::Bool(flag));
        }
        if let Some(vol) = volume {
            ent.insert("volume".into(), Value::from(vol.min(100)));
        }
    }
    write_state(cache_dir, &state);
}

fn group_key(ent: &Value) -> Option<(String, String, String)> {
    let ty = ent.get("type").and_then(Value::as_str).unwrap_or("").to_string();
    if ty != wall_proto::kind::VIDEO && ty != wall_proto::kind::WE {
        return None;
    }
    let path = ent.get("path").and_then(Value::as_str).unwrap_or("").to_string();
    let we_id = ent.get("we_id").and_then(Value::as_str).unwrap_or("").to_string();
    Some((ty, path, we_id))
}

fn is_muted(ent: &Value) -> bool {
    ent.get("mute").and_then(Value::as_bool).unwrap_or(true)
}

pub fn video_audio_groups(state: &Value) -> Vec<(Vec<String>, bool, u32)> {
    let Some(map) = state.as_object() else {
        return Vec::new();
    };
    let mut grouped = std::collections::BTreeMap::<String, Vec<(String, bool, u32)>>::new();
    for (output, ent) in map {
        if ent.get("type").and_then(Value::as_str) != Some(wall_proto::kind::VIDEO) {
            continue;
        }
        let path = ent.get("path").and_then(Value::as_str).unwrap_or("");
        if path.is_empty() {
            continue;
        }
        grouped.entry(path.to_string()).or_default().push((
            output.clone(),
            is_muted(ent),
            ent.get("volume").and_then(Value::as_u64).map_or(100, |vol| (vol as u32).min(100)),
        ));
    }
    grouped
        .into_values()
        .map(|mut entries| {
            entries.sort_by(|lhs, rhs| lhs.0.cmp(&rhs.0));
            let effective = entries
                .iter()
                .find(|(_, muted, _)| !muted)
                .or_else(|| entries.first())
                .expect("video audio group is non-empty");
            let mute = effective.1;
            let volume = effective.2;
            let outputs = entries.into_iter().map(|(output, _, _)| output).collect();
            (outputs, mute, volume)
        })
        .collect()
}

pub fn compute_dedup(state: &Value) -> Vec<String> {
    let Some(map) = state.as_object() else {
        return Vec::new();
    };
    let mut groups: HashMap<(String, String, String), Vec<String>> = HashMap::new();
    for (out, ent) in map {
        if let Some(key) = group_key(ent) {
            groups.entry(key).or_default().push(out.clone());
        }
    }
    let mut to_mute = Vec::new();
    for (_, mut outs) in groups {
        outs.sort();
        let mut found_primary = false;
        for out in &outs {
            let muted = map.get(out).is_none_or(is_muted);
            if !muted {
                if found_primary {
                    to_mute.push(out.clone());
                } else {
                    found_primary = true;
                }
            }
        }
    }
    to_mute.sort();
    to_mute
}

pub fn compute_dedup_from<S: BuildHasher>(
    existing: &Value,
    target_outputs: &[String],
    audio: &HashMap<String, bool, S>,
    ty: &str,
    path: &str,
    we_id: &str,
    global_mute: bool,
) -> HashMap<String, bool> {
    let mut winner_chosen = external_unmuted(existing, target_outputs, ty, path, we_id);
    let mut sorted = target_outputs.to_vec();
    sorted.sort();
    let mut result = HashMap::new();
    for out in sorted {
        let desired_mute = audio.get(&out).copied().unwrap_or(global_mute);
        if !desired_mute && !winner_chosen {
            winner_chosen = true;
            result.insert(out, false);
        } else {
            result.insert(out, true);
        }
    }
    result
}

fn external_unmuted(
    existing: &Value,
    target_outputs: &[String],
    ty: &str,
    path: &str,
    we_id: &str,
) -> bool {
    let Some(map) = existing.as_object() else {
        return false;
    };
    for (out, ent) in map {
        if target_outputs.iter().any(|name| name == out) {
            continue;
        }
        let Some((et, ep, ewe)) = group_key(ent) else {
            continue;
        };
        let same = if ty == wall_proto::kind::WE {
            et == wall_proto::kind::WE && ewe == we_id
        } else {
            et == ty && ep == path
        };
        if same && !is_muted(ent) {
            return true;
        }
    }
    false
}

pub fn compute_preserve(prev_state: &Value, current_state: &Value) -> Vec<String> {
    let prev_audible = audible_groups(prev_state);
    if prev_audible.is_empty() {
        return Vec::new();
    }
    let Some(map) = current_state.as_object() else {
        return Vec::new();
    };
    let mut groups: HashMap<(String, String, String), Vec<(String, bool)>> = HashMap::new();
    for (out, ent) in map {
        if let Some(key) = group_key(ent) {
            groups.entry(key).or_default().push((out.clone(), is_muted(ent)));
        }
    }
    let mut to_unmute = Vec::new();
    for (key, mut outs) in groups {
        if !prev_audible.contains(&key) {
            continue;
        }
        if outs.iter().any(|(_, muted)| !muted) {
            continue;
        }
        outs.sort_by(|lhs, rhs| lhs.0.cmp(&rhs.0));
        if let Some((out, _)) = outs.first() {
            to_unmute.push(out.clone());
        }
    }
    to_unmute.sort();
    to_unmute
}

fn audible_groups(state: &Value) -> std::collections::HashSet<(String, String, String)> {
    let mut set = std::collections::HashSet::new();
    let Some(obj) = state.as_object() else {
        return set;
    };
    for ent in obj.values() {
        if is_muted(ent) {
            continue;
        }
        if let Some(key) = group_key(ent) {
            set.insert(key);
        }
    }
    set
}

#[path = "tests.rs"]
mod tests;

pub fn mute_dedup_losers(state: &crate::state::WallState, cache: &str) {
    let losers = compute_dedup(&read_state(cache));
    if !losers.is_empty() {
        state.renderers().send_audio(Some(&losers), Some(true), None);
        update_audio(cache, Some(&losers), Some(true), None);
    }
    // One pipeline per source: a targeted loser-mute also hits its winner, so resend both.
    for (outputs, mute, volume) in video_audio_groups(&read_state(cache)) {
        state.renderers().send_multi_video_audio(&outputs, mute, volume);
    }
}
