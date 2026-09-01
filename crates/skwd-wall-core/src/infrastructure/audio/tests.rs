#![cfg(test)]

use super::*;
use serde_json::json;

fn st(entries: &[(&str, &str, &str, &str, bool)]) -> Value {
    let mut map = Map::new();
    for (out, ty, path, we_id, mute) in entries {
        map.insert(
            (*out).to_string(),
            json!({"type": ty, "path": path, "we_id": we_id, "mute": mute, "volume": 100}),
        );
    }
    Value::Object(map)
}

#[test]
fn resolve_defaults_precedence() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = tmp.path().to_string_lossy().into_owned();
    assert_eq!(resolve_defaults(&cache, "DP-1", true, 100), (true, 100));
    let mut map = Map::new();
    map.insert("*".into(), json!({"mute": false, "volume": 30}));
    write_state(&cache, &Value::Object(map.clone()));
    assert_eq!(resolve_defaults(&cache, "DP-1", true, 100), (false, 30));
    assert_eq!(resolve_defaults(&cache, "*", true, 100), (false, 30));
    map.insert("DP-1".into(), json!({"mute": true, "volume": 45}));
    write_state(&cache, &Value::Object(map));
    assert_eq!(resolve_defaults(&cache, "DP-1", false, 100), (true, 45));
}

#[test]
fn star_follows_audible() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = tmp.path().to_string_lossy().into_owned();
    let mut map = Map::new();
    map.insert("DP-2".into(), json!({"mute": false, "volume": 45}));
    map.insert("DP-1".into(), json!({"mute": true, "volume": 70}));
    write_state(&cache, &Value::Object(map.clone()));
    assert_eq!(resolve_defaults(&cache, "*", true, 0), (false, 45),);
    map.insert("DP-2".into(), json!({"mute": true, "volume": 45}));
    write_state(&cache, &Value::Object(map));
    assert_eq!(resolve_defaults(&cache, "*", false, 0), (true, 70),);
}

#[test]
fn mixer_survives_apply() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = tmp.path().to_string_lossy().into_owned();
    record_outputs(&cache, &[], "video", "/a.mp4", "", &HashMap::new(), &HashMap::new(), false, 0);
    let names = vec!["DP-1".to_string(), "DP-2".to_string()];
    expand_wildcard(&cache, &names);
    update_audio(&cache, Some(&["DP-1".to_string()]), Some(false), Some(45));
    let state = read_state(&cache);
    assert_eq!(state["DP-1"]["volume"], json!(45));
    assert_eq!(state["DP-2"]["volume"], json!(0));
    let (mute, volume) = resolve_defaults(&cache, "*", true, 0);
    assert_eq!((mute, volume), (false, 45));
    record_outputs(
        &cache,
        &[],
        "video",
        "/b.mp4",
        "",
        &HashMap::new(),
        &HashMap::new(),
        mute,
        volume,
    );
    assert_eq!(read_state(&cache)["*"]["volume"], json!(45),);
}

#[test]
fn carried_audio_moves() {
    let mut map = Map::new();
    map.insert("DP-1".into(), json!({"mute": false, "volume": 45}));
    map.insert("*".into(), json!({"mute": true, "volume": 20}));
    let prev = Value::Object(map);
    assert_eq!(carried_audio(&prev, "DP-1", true, 0), (false, 45));
    assert_eq!(carried_audio(&prev, "DP-2", true, 0), (true, 20));
    assert_eq!(carried_audio(&Value::Object(Map::new()), "DP-1", true, 7), (true, 7));
    assert_eq!(carried_audio(&prev, "DP-1", true, 0), (false, 45));
    let ent = json!({"mute": false, "volume": 400});
    let mut map2 = Map::new();
    map2.insert("DP-1".into(), ent);
    assert_eq!(carried_audio(&Value::Object(map2), "DP-1", true, 0).1, 100);
}

#[test]
fn groups_pick_audible_owner() {
    let state = serde_json::json!({
        "DP-2": entry("video", "/v/a.mp4", "", true, 10),
        "DP-1": entry("video", "/v/a.mp4", "", false, 37),
        "DP-3": entry("video", "/v/b.mp4", "", true, 82),
        "DP-4": entry("static", "/w/c.png", "", false, 99),
    });
    assert_eq!(
        video_audio_groups(&state),
        vec![
            (vec!["DP-1".to_string(), "DP-2".to_string()], false, 37),
            (vec!["DP-3".to_string()], true, 82),
        ]
    );
}

#[test]
fn dedup_keeps_first() {
    let state =
        st(&[("DP-2", "video", "/v.mp4", "", false), ("DP-1", "video", "/v.mp4", "", false)]);
    assert_eq!(compute_dedup(&state), vec!["DP-2".to_string()]);
}

#[test]
fn dedup_muted_noop() {
    let state = st(&[("DP-1", "video", "/v.mp4", "", true), ("DP-2", "video", "/v.mp4", "", true)]);
    assert!(compute_dedup(&state).is_empty());
}

#[test]
fn dedup_distinct_sources() {
    let state =
        st(&[("DP-1", "video", "/a.mp4", "", false), ("DP-2", "video", "/b.mp4", "", false)]);
    assert!(compute_dedup(&state).is_empty());
}

#[test]
fn dedup_video_we_independent() {
    let state = serde_json::json!({
        "DP-1": entry("video", "/shared/100", "", false, 31),
        "DP-2": entry("we", "", "100", false, 47),
    });
    assert!(compute_dedup(&state).is_empty());
    assert_eq!(video_audio_groups(&state), vec![(vec!["DP-1".to_string()], false, 31)]);
}

#[test]
fn dedup_skips_static() {
    let mut map = Map::new();
    map.insert(
        "DP-1".into(),
        json!({"type": "static", "path": "/w.png", "we_id": "", "mute": false}),
    );
    map.insert(
        "DP-2".into(),
        json!({"type": "static", "path": "/w.png", "we_id": "", "mute": false}),
    );
    assert!(compute_dedup(&Value::Object(map)).is_empty());
}

#[test]
fn dedup_from_first_wins() {
    let res = compute_dedup_from(
        &Value::Object(Map::new()),
        &["DP-2".into(), "DP-1".into()],
        &HashMap::new(),
        "video",
        "/v.mp4",
        "",
        false,
    );
    assert_eq!(res.get("DP-1"), Some(&false));
    assert_eq!(res.get("DP-2"), Some(&true));
}

#[test]
fn dedup_from_all_muted() {
    let res = compute_dedup_from(
        &Value::Object(Map::new()),
        &["DP-1".into(), "DP-2".into()],
        &HashMap::new(),
        "video",
        "/v.mp4",
        "",
        true,
    );
    assert_eq!(res.get("DP-1"), Some(&true));
    assert_eq!(res.get("DP-2"), Some(&true));
}

#[test]
fn override_beats_mute() {
    let mut audio = HashMap::new();
    audio.insert("DP-2".to_string(), false);
    let res = compute_dedup_from(
        &Value::Object(Map::new()),
        &["DP-1".into(), "DP-2".into()],
        &audio,
        "video",
        "/v.mp4",
        "",
        true,
    );
    assert_eq!(res.get("DP-1"), Some(&true));
    assert_eq!(res.get("DP-2"), Some(&false));
}

#[test]
fn dedup_defers_external() {
    let existing = st(&[("DP-3", "static", "/w.png", "", false)]);
    let mut existing = existing;
    existing.as_object_mut().unwrap().insert(
        "DP-9".into(),
        json!({"type": "video", "path": "/v.mp4", "we_id": "", "mute": false, "volume": 100}),
    );
    let res = compute_dedup_from(
        &existing,
        &["DP-1".into(), "DP-2".into()],
        &HashMap::new(),
        "video",
        "/v.mp4",
        "",
        false,
    );
    assert_eq!(res.get("DP-1"), Some(&true));
    assert_eq!(res.get("DP-2"), Some(&true));
}

#[test]
fn preserve_reunmutes_first() {
    let prev = st(&[("DP-1", "video", "/v.mp4", "", false)]);
    let current =
        st(&[("DP-1", "video", "/v.mp4", "", true), ("DP-2", "video", "/v.mp4", "", true)]);
    assert_eq!(compute_preserve(&prev, &current), vec!["DP-1".to_string()]);
}

#[test]
fn preserve_noop() {
    let prev = st(&[("DP-1", "video", "/v.mp4", "", false)]);
    let current = st(&[("DP-2", "video", "/v.mp4", "", false)]);
    assert!(compute_preserve(&prev, &current).is_empty());
}

#[test]
fn expand_then_partial() {
    let dir = tempfile::tempdir().unwrap();
    let cd = dir.path().to_str().unwrap();
    record_outputs(cd, &[], "video", "/v.mp4", "", &HashMap::new(), &HashMap::new(), false, 80);
    assert!(read_state(cd).get("*").is_some());
    expand_wildcard(cd, &["DP-1".to_string(), "DP-2".to_string()]);
    let state = read_state(cd);
    assert!(state.get("*").is_none());
    assert_eq!(state["DP-1"]["path"], json!("/v.mp4"));
    assert_eq!(state["DP-2"]["path"], json!("/v.mp4"));
    set_entry(cd, "DP-1", "video", "/new.mp4", "", false, 80);
    let state = read_state(cd);
    assert_eq!(state["DP-1"]["path"], json!("/new.mp4"));
    assert_eq!(state["DP-2"]["path"], json!("/v.mp4"));
}

#[test]
fn expand_keeps_existing() {
    let dir = tempfile::tempdir().unwrap();
    let cd = dir.path().to_str().unwrap();
    set_entry(cd, "DP-1", "static", "/a.png", "", true, 0);
    set_entry(cd, "*", "video", "/v.mp4", "", false, 80);
    expand_wildcard(cd, &["DP-1".to_string(), "DP-2".to_string()]);
    let state = read_state(cd);
    assert_eq!(state["DP-1"]["type"], json!("static"));
    assert_eq!(state["DP-2"]["type"], json!("video"));
}

#[test]
fn read_state_corrupt() {
    let dir = tempfile::tempdir().unwrap();
    let cd = dir.path().to_str().unwrap();
    assert_eq!(read_state(cd), Value::Object(Map::new()));
    for bad in ["{\"DP-1\": tru", "", "[1,2]", "\"str\"", "42", "null"] {
        std::fs::write(state_path(cd), bad).unwrap();
        let state = read_state(cd);
        assert_eq!(state, Value::Object(Map::new()));
        assert!(compute_dedup(&state).is_empty());
        assert!(compute_preserve(&state, &state).is_empty());
    }
    std::fs::write(state_path(cd), "[1,2]").unwrap();
    update_audio(cd, None, Some(true), None);
    set_entry(cd, "DP-1", "video", "/v.mp4", "", false, 70);
    let state = read_state(cd);
    assert_eq!(state["DP-1"]["path"], json!("/v.mp4"));
    assert_eq!(state["DP-1"]["volume"], json!(70));
}

#[test]
fn write_state_no_tmp() {
    let dir = tempfile::tempdir().unwrap();
    let cd = dir.path().join("nested").join("cache");
    let cds = cd.to_str().unwrap();
    let state = st(&[("DP-1", "video", "/v.mp4", "", false)]);
    write_state(cds, &state);
    assert_eq!(read_state(cds), state);
    let leftovers: Vec<String> = std::fs::read_dir(&cd)
        .unwrap()
        .filter_map(std::result::Result::ok)
        .map(|ent| ent.file_name().to_string_lossy().into_owned())
        .filter(|name| name != "outputs.json")
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
fn entry_clamps_volume() {
    let ent = entry("video", "/v.mp4", "", false, 250);
    assert_eq!(ent["volume"], json!(100));
    assert_eq!(ent["type"], json!("video"));
}

#[test]
fn state_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let cd = dir.path().to_str().unwrap();
    let mut mute = HashMap::new();
    mute.insert("DP-1".to_string(), false);
    record_outputs(
        cd,
        &["DP-1".into(), "DP-2".into()],
        "video",
        "/v.mp4",
        "",
        &mute,
        &HashMap::new(),
        true,
        90,
    );
    let state = read_state(cd);
    assert_eq!(state["DP-1"]["mute"], json!(false));
    assert_eq!(state["DP-2"]["mute"], json!(true));
    assert_eq!(state["DP-1"]["volume"], json!(90));
    update_audio(cd, Some(&["DP-2".to_string()]), Some(false), Some(55));
    let state2 = read_state(cd);
    assert_eq!(state2["DP-2"]["mute"], json!(false));
    assert_eq!(state2["DP-2"]["volume"], json!(55));
    assert_eq!(state2["DP-1"]["volume"], json!(90));
}
