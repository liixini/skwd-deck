#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchParams {
    pub query: String,
    pub query_type: u64,
    pub trend_days: u32,
    pub page: u32,
    pub required_tags: Vec<String>,
    pub excluded_tags: Vec<String>,
}

impl SearchParams {
    pub fn decode(value: &serde_json::Value) -> Self {
        Self {
            query: value.get("query").and_then(serde_json::Value::as_str).unwrap_or("").to_string(),
            query_type: value.get("query_type").and_then(serde_json::Value::as_u64).unwrap_or(3),
            trend_days: value
                .get("days")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(7)
                .clamp(1, 7) as u32,
            page: value.get("page").and_then(serde_json::Value::as_u64).unwrap_or(1).max(1) as u32,
            required_tags: tags(value, "tags"),
            excluded_tags: tags(value, "excluded_tags"),
        }
    }
}

fn tags(value: &serde_json::Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map(|array| array.iter().filter_map(|tag| tag.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "search_params_tests.rs"]
mod tests;
