use serde_json::{Map, Value, json};

use crate::{keys, schema};

pub const KEYS: [&str; 14] = [
    keys::theme::POLICY,
    keys::theme::AUTHORITY,
    keys::theme::ENGINE,
    keys::theme::MODE,
    keys::theme::STATIC_THEME,
    keys::theme::STYLE,
    keys::theme::SCHEME,
    keys::theme::WALLUST_PALETTE,
    keys::theme::WALLUST_COLORSPACE,
    keys::theme::PYWAL_SATURATE,
    keys::theme::NOCTALIA_SCHEME,
    keys::theme::NOCTALIA_PURE_BLACK,
    keys::matugen::SCHEME_TYPE,
    keys::matugen::COLOR_INDEX,
];

pub fn snapshot(root: &Value) -> Map<String, Value> {
    KEYS.into_iter()
        .filter_map(|path| {
            let value = match path {
                keys::theme::POLICY => json!(crate::theme_policy(root)),
                keys::theme::AUTHORITY => json!(crate::theme_authority(root)),
                keys::theme::ENGINE => json!(crate::theme_engine(root)),
                _ => match schema::value_kind(path)? {
                    schema::ValueKind::Boolean => json!(schema::read_boolean(root, path)?),
                    schema::ValueKind::Number => {
                        schema::normalize_value(path, &json!(schema::read_number(root, path)?))?
                    }
                    schema::ValueKind::Text => json!(schema::read_text(root, path)?),
                    _ => return None,
                },
            };
            Some((path.to_string(), value))
        })
        .collect()
}

pub fn validated(settings: &Map<String, Value>) -> Map<String, Value> {
    KEYS.into_iter()
        .filter_map(|path| {
            let value = schema::normalize_value(path, settings.get(path)?)?;
            Some((path.to_string(), value))
        })
        .collect()
}

pub fn settings(profiles: &[Value], key: &str) -> Option<Map<String, Value>> {
    profiles
        .iter()
        .find(|profile| profile["key"].as_str() == Some(key) && profile["settingsPinned"] == true)
        .and_then(|profile| profile.get("settings")?.as_object())
        .map(validated)
}

#[cfg(test)]
#[path = "theme_profile_tests.rs"]
mod tests;
