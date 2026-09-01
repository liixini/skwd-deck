use serde_json::Value;

pub fn get<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = root;
    for part in path.split('.') {
        cur = if cur.is_array() { cur.get(part.parse::<usize>().ok()?)? } else { cur.get(part)? };
    }
    Some(cur)
}

pub fn str_at(root: &Value, path: &str, default: &str) -> String {
    get(root, path).and_then(Value::as_str).map_or_else(|| default.into(), String::from)
}

pub fn str_ref<'a>(root: &'a Value, path: &str, default: &'a str) -> &'a str {
    get(root, path).and_then(Value::as_str).unwrap_or(default)
}

pub fn i64_ref(root: &Value, path: &str, default: i64) -> i64 {
    get(root, path).and_then(Value::as_i64).unwrap_or(default)
}

pub fn u64_ref(root: &Value, path: &str, default: u64) -> u64 {
    get(root, path).and_then(Value::as_u64).unwrap_or(default)
}

pub fn f64_ref(root: &Value, path: &str, default: f64) -> f64 {
    get(root, path).and_then(Value::as_f64).unwrap_or(default)
}

pub fn arr_ref<'a>(root: &'a Value, path: &str) -> &'a [Value] {
    get(root, path).and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

pub fn num_at(root: &Value, path: &str, default: f64) -> f64 {
    get(root, path)
        .and_then(|val| {
            val.as_f64().or_else(|| val.as_str().and_then(|text| text.trim().parse().ok()))
        })
        .unwrap_or(default)
}

pub fn u64_at(root: &Value, path: &str) -> Option<u64> {
    get(root, path).and_then(|val| {
        val.as_u64()
            .or_else(|| val.as_f64().map(|num| num as u64))
            .or_else(|| val.as_str().and_then(|text| text.trim().parse().ok()))
    })
}

pub fn bool_true_unless_false(root: &Value, path: &str) -> bool {
    get(root, path).and_then(Value::as_bool) != Some(false)
}

pub fn bool_false_unless_true(root: &Value, path: &str) -> bool {
    get(root, path).and_then(Value::as_bool) == Some(true)
}

pub fn bool_at(root: &Value, path: &str, default: bool) -> bool {
    get(root, path).and_then(Value::as_bool).unwrap_or(default)
}

#[cfg(test)]
#[path = "value_tests.rs"]
mod tests;
