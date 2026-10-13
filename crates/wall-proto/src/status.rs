use serde_json::{Value, json};

pub const PROTOCOL_NAME: &str = "skwd-wall";
pub const PROTOCOL_VERSION: u32 = 1;
pub const SERVICE_NAME: &str = "skwd-deck";
pub const SERVICE_COMPONENT: &str = "skwd-walld";
pub const STEAM_CLIENT_AVAILABLE_FIELD: &str = "steam_client_available";
pub const CAPABILITIES: &[&str] = &[
    "effects",
    "events",
    "history",
    "picker-session",
    "playlists",
    "schedules",
    "sources",
    "tasks",
    "wallpapers",
    "weather-playlists",
];

pub fn deck_status(version: &str) -> Value {
    json!({
        "ok": true,
        "version": version,
        "service": {
            "name": SERVICE_NAME,
            "component": SERVICE_COMPONENT,
        },
        "protocol": {
            "name": PROTOCOL_NAME,
            "version": PROTOCOL_VERSION,
        },
        "capabilities": CAPABILITIES,
    })
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
