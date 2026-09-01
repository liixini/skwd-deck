use serde_json::Value;

use super::filter::{Item, matches};

pub fn tag_tokens(tags: &str) -> Vec<String> {
    let tags = tags.trim();
    if tags.starts_with('[')
        && let Ok(Value::Array(values)) = serde_json::from_str::<Value>(tags)
    {
        return values
            .iter()
            .filter_map(Value::as_str)
            .map(|tag| tag.trim().to_lowercase())
            .filter(|tag| !tag.is_empty())
            .collect();
    }
    tags.split([',', ' '])
        .map(|tag| tag.trim().to_lowercase())
        .filter(|tag| !tag.is_empty())
        .collect()
}

pub fn matches_item(item: &Value, source: &str) -> bool {
    let tags = tag_tokens(item.get("tags").and_then(Value::as_str).unwrap_or(""));
    matches(
        &Item {
            key: item.get("key").and_then(Value::as_str).unwrap_or(""),
            tags: &tags,
            kind: item.get("type").and_then(Value::as_str).unwrap_or(""),
            hue: item.get("hue").and_then(Value::as_i64).unwrap_or(99),
            width: item.get("width").and_then(Value::as_i64).unwrap_or(0),
            height: item.get("height").and_then(Value::as_i64).unwrap_or(0),
        },
        source,
    )
}

#[cfg(test)]
#[path = "json_item_tests.rs"]
mod tests;
