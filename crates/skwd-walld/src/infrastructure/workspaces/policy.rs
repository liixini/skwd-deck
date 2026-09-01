use std::collections::HashMap;
use std::time::Instant;

use serde_json::Value;

use super::model::{
    BaseWallpaper, DesiredWallpaper, WorkspaceInfo, WorkspaceRule, WorkspaceRuntime,
};

pub(super) fn parse_rule(value: &Value) -> Option<WorkspaceRule> {
    let wallpaper =
        value.get("wallpaper").and_then(Value::as_str).filter(|text| !text.is_empty())?.to_string();
    let output = value.get("output").and_then(Value::as_str).unwrap_or("").to_string();
    let matcher = value
        .get("workspace")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| value.get("workspace").and_then(Value::as_u64).map(|number| number.to_string()))
        .filter(|text| !text.is_empty())?;
    Some(WorkspaceRule { output, matcher, wallpaper })
}

pub(super) fn parse_rules(values: &[Value]) -> Vec<WorkspaceRule> {
    values.iter().filter_map(parse_rule).collect()
}

pub(super) fn match_rule<'a>(rules: &'a [WorkspaceRule], info: &WorkspaceInfo) -> Option<&'a str> {
    rules
        .iter()
        .find(|rule| {
            (rule.output.is_empty() || rule.output == info.output)
                && (info.name.as_deref() == Some(rule.matcher.as_str())
                    || rule.matcher == info.idx.to_string())
        })
        .map(|rule| rule.wallpaper.as_str())
}

pub(super) fn mark_active(
    topology: &mut HashMap<u64, WorkspaceInfo>,
    id: u64,
) -> Option<(String, &'static str)> {
    let (output, new_index) =
        topology.get(&id).map(|workspace| (workspace.output.clone(), workspace.idx))?;
    let old_index = topology
        .values()
        .find(|workspace| workspace.output == output && workspace.active)
        .map(|workspace| workspace.idx);
    for (workspace_id, workspace) in topology.iter_mut() {
        if workspace.output == output {
            workspace.active = *workspace_id == id;
        }
    }
    let old_index = old_index?;
    if old_index == new_index {
        return None;
    }
    Some((output, if new_index > old_index { "up" } else { "down" }))
}

pub(super) fn compute_updates(
    runtime: &WorkspaceRuntime,
    rules: &[WorkspaceRule],
) -> Vec<(String, DesiredWallpaper)> {
    let mut active: HashMap<&str, &WorkspaceInfo> = HashMap::new();
    for info in runtime.topo.values() {
        if info.active {
            active.insert(info.output.as_str(), info);
        }
    }
    active
        .into_iter()
        .filter_map(|(output, info)| {
            if let Some(wallpaper) = match_rule(rules, info) {
                let stale = runtime.last.get(output).map(String::as_str) != Some(wallpaper)
                    && runtime.pending.get(output)
                        != Some(&DesiredWallpaper::Pin(wallpaper.to_string()));
                return stale
                    .then(|| (output.to_string(), DesiredWallpaper::Pin(wallpaper.to_string())));
            }
            let showing_pin = runtime.last.contains_key(output);
            let restorable = runtime.base.contains_key(output);
            let queued = runtime.pending.get(output) == Some(&DesiredWallpaper::Base);
            (showing_pin && restorable && !queued)
                .then(|| (output.to_string(), DesiredWallpaper::Base))
        })
        .collect()
}

pub(super) fn refresh_pending(
    runtime: &mut WorkspaceRuntime,
    rules: &[WorkspaceRule],
    deadline: Instant,
) -> bool {
    let updates = compute_updates(runtime, rules);
    if updates.is_empty() {
        return false;
    }
    for (output, wallpaper) in updates {
        runtime.pending.insert(output, wallpaper);
    }
    runtime.deadline = Some(deadline);
    true
}

pub(super) fn preload_paths(
    rules: &[WorkspaceRule],
    base: Option<&BaseWallpaper>,
    output: &str,
    wallpaper_dir: &str,
    video_dir: &str,
) -> Vec<String> {
    let mut paths = Vec::new();
    if let Some(entry) = base
        && entry.ty == wall_proto::kind::STATIC
        && !entry.path.is_empty()
    {
        paths.push(entry.path.clone());
    }
    for rule in rules {
        if !rule.output.is_empty() && rule.output != output {
            continue;
        }
        let (kind, path, _) =
            crate::composition::apply::key_apply_args(&rule.wallpaper, wallpaper_dir, video_dir);
        if kind == wall_proto::kind::STATIC && !paths.contains(&path) {
            paths.push(path);
        }
        if paths.len() >= 8 {
            break;
        }
    }
    paths
}
