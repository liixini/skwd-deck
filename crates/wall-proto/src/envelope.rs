use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
    #[serde(default)]
    pub id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorInfo {
    pub code: i32,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub event: String,
    #[serde(default)]
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ServerMessage {
    Response(Response),
    Event(Event),
}

impl Request {
    pub fn str_param<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.params.get(key).and_then(|value| value.as_str()).unwrap_or(default)
    }

    pub fn opt_str(&self, key: &str) -> Option<&str> {
        self.params.get(key).and_then(|value| value.as_str())
    }

    pub fn bool_param(&self, key: &str, default: bool) -> bool {
        self.params.get(key).and_then(serde_json::Value::as_bool).unwrap_or(default)
    }

    pub fn opt_i64(&self, key: &str) -> Option<i64> {
        self.params.get(key).and_then(serde_json::Value::as_i64)
    }

    pub fn opt_u64(&self, key: &str) -> Option<u64> {
        self.params.get(key).and_then(serde_json::Value::as_u64)
    }

    pub fn str_array(&self, key: &str) -> Vec<String> {
        self.params
            .get(key)
            .and_then(|value| value.as_array())
            .map(|array| array.iter().filter_map(|item| item.as_str().map(String::from)).collect())
            .unwrap_or_default()
    }
}

impl Response {
    pub fn ok(id: u64, result: serde_json::Value) -> Self {
        Self { id, result: Some(result), error: None }
    }

    pub fn err(id: u64, code: i32, message: impl Into<String>) -> Self {
        let message = crate::redact_sensitive(&message.into());
        Self { id, result: None, error: Some(ErrorInfo { code, message }) }
    }
}

#[cfg(test)]
#[path = "envelope_tests.rs"]
mod tests;
