use std::collections::BTreeMap;

fn resolve_sibling(path: &str, exts: &[&str]) -> String {
    let fpath = std::path::Path::new(path);
    if fpath.exists() {
        return path.to_string();
    }
    let (Some(dir), Some(stem)) = (fpath.parent(), fpath.file_stem()) else {
        return path.to_string();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return path.to_string();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|candidate| {
            candidate.file_stem() == Some(stem)
                && candidate
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| exts.contains(&ext.to_ascii_lowercase().as_str()))
        })
        .map_or_else(|| path.to_string(), |candidate| candidate.to_string_lossy().into_owned())
}

fn resolve_dir_preview(path: &str) -> Option<String> {
    let dir = std::path::Path::new(path);
    if !dir.is_dir() {
        return None;
    }
    crate::we::find_preview(dir).map(|preview| preview.display().to_string())
}

pub fn resolve_current_image(path: &str) -> String {
    resolve_dir_preview(path)
        .unwrap_or_else(|| resolve_sibling(path, &["webp", "jpg", "jpeg", "png", "avif", "gif"]))
}

pub fn resolve_current_video(path: &str) -> String {
    resolve_dir_preview(path).unwrap_or_else(|| resolve_sibling(path, paper_control::VIDEO_EXTS))
}

pub fn resolve_we_from_state(
    map: &serde_json::Map<String, serde_json::Value>,
) -> (BTreeMap<String, Vec<String>>, BTreeMap<String, (bool, u32)>) {
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (output, entry) in map {
        if entry.get("type").and_then(serde_json::Value::as_str) != Some(wall_proto::kind::WE) {
            continue;
        }
        let we_id = entry.get("we_id").and_then(serde_json::Value::as_str).unwrap_or("");
        if !we_id.is_empty() {
            groups.entry(we_id.to_string()).or_default().push(output.clone());
        }
    }
    for outputs in groups.values_mut() {
        outputs.sort();
    }
    let audio = resolve_we_audio(map, &groups);
    (groups, audio)
}

pub(super) fn resolve_we_audio(
    map: &serde_json::Map<String, serde_json::Value>,
    groups: &BTreeMap<String, Vec<String>>,
) -> BTreeMap<String, (bool, u32)> {
    groups
        .iter()
        .map(|(we_id, outputs)| {
            let audible = outputs.iter().find_map(|output| {
                let entry = map.get(output)?;
                (!entry.get("mute").and_then(serde_json::Value::as_bool).unwrap_or(true)).then(
                    || {
                        entry
                            .get("volume")
                            .and_then(serde_json::Value::as_u64)
                            .unwrap_or(100)
                            .min(100) as u32
                    },
                )
            });
            (we_id.clone(), audible.map_or((true, 100), |volume| (false, volume)))
        })
        .collect()
}

pub(super) fn reconcile_targets(
    monitors: &[String],
    map: &serde_json::Map<String, serde_json::Value>,
) -> Vec<String> {
    let mut targets: Vec<String> = monitors.to_vec();
    for key in map.keys() {
        if key != "*" && !targets.iter().any(|monitor| monitor == key) {
            targets.push(key.clone());
        }
    }
    targets
}
