pub fn options() -> Vec<serde_json::Value> {
    crate::gowall::names()
        .into_iter()
        .map(|name| serde_json::json!({ "mode": name, "label": name }))
        .collect()
}

#[cfg(test)]
#[path = "gowall_json_tests.rs"]
mod tests;
