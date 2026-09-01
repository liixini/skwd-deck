use serde_json::{Map, Value};
use wall_proto::{WeProperty, WePropertyOption, we_property_kind as kind};

use crate::state::WallState;

pub fn scene_properties(state: &WallState, we_id: &str) -> Vec<WeProperty> {
    if !super::valid_we_id(we_id) {
        return Vec::new();
    }
    let item_dir = state.config().we_dir().join(we_id);
    let declared = read_declarations(&item_dir);
    let overrides = super::scene_overrides(state, we_id);
    merge(&declared, &overrides)
}

pub fn read_declarations(item_dir: &std::path::Path) -> Map<String, Value> {
    let Ok(bytes) = std::fs::read(item_dir.join("project.json")) else {
        return Map::new();
    };
    let Ok(project) = serde_json::from_slice::<Value>(&strip_bom(&bytes)) else {
        return Map::new();
    };
    project
        .get("general")
        .and_then(|top| top.get("properties"))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn strip_bom(bytes: &[u8]) -> Vec<u8> {
    bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes).to_vec()
}

pub fn merge(declared: &Map<String, Value>, overrides: &Map<String, Value>) -> Vec<WeProperty> {
    let mut rows: Vec<WeProperty> = declared
        .iter()
        .filter_map(|(name, entry)| entry.as_object().map(|entry| row(name, entry, overrides)))
        .collect();
    rows.sort_by(|lhs, rhs| lhs.order.cmp(&rhs.order).then_with(|| lhs.name.cmp(&rhs.name)));
    rows
}

fn row(name: &str, entry: &Map<String, Value>, overrides: &Map<String, Value>) -> WeProperty {
    let declared = entry.get("type").and_then(Value::as_str).unwrap_or("").to_string();
    let default = entry.get("value").cloned().unwrap_or(Value::Null);
    let resolved = resolve_kind(&declared, entry, &default);
    let override_value = overrides.get(name);
    WeProperty {
        name: name.to_string(),
        label: entry.get("text").and_then(Value::as_str).unwrap_or(name).trim().to_string(),
        kind: resolved.to_string(),
        declared,
        value: override_value.cloned().unwrap_or_else(|| default.clone()),
        default,
        overridden: override_value.is_some(),
        min: number(entry.get("min")),
        max: number(entry.get("max")),
        step: number(entry.get("step")),
        options: options(entry.get("options")),
        condition: entry.get("condition").and_then(Value::as_str).map(str::to_string),
        order: entry.get("order").and_then(Value::as_i64).unwrap_or(i64::MAX),
    }
}

fn resolve_kind(declared: &str, entry: &Map<String, Value>, default: &Value) -> &'static str {
    match declared.to_ascii_lowercase().as_str() {
        "bool" => kind::BOOL,
        "color" => kind::COLOR,
        "slider" => kind::SLIDER,
        "combo" => kind::COMBO,
        "group" => kind::GROUP,
        "" => infer(entry, default),
        _ => kind::UNSUPPORTED,
    }
}

fn infer(entry: &Map<String, Value>, default: &Value) -> &'static str {
    if entry.contains_key("options") {
        return kind::COMBO;
    }
    if entry.contains_key("min") && entry.contains_key("max") {
        return kind::SLIDER;
    }
    match default {
        Value::Bool(_) => kind::BOOL,
        Value::Number(_) => kind::SLIDER,
        Value::String(text) if text.split_whitespace().count() == 3 => kind::COLOR,
        _ => kind::UNSUPPORTED,
    }
}

fn number(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(num) => num.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

fn options(value: Option<&Value>) -> Vec<WePropertyOption> {
    let Some(Value::Array(entries)) = value else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(Value::as_object)
        .map(|entry| WePropertyOption {
            label: entry.get("label").and_then(Value::as_str).unwrap_or("").trim().to_string(),
            value: number(entry.get("value")).unwrap_or_default(),
        })
        .collect()
}

#[cfg(test)]
#[path = "properties_tests.rs"]
mod tests;
