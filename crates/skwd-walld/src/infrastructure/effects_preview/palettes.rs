use serde_json::{Value, json};
use skwd_wall_core::config::Config;

fn colours(theme: &Value) -> Vec<String> {
    let mut colours = Vec::new();
    let mut add = |value: &Value| {
        if let Some(hex) = value.as_str()
            && hex.starts_with('#')
            && hex.len() == 7
            && let Some(rgb) = skwd_palette::parse_hex(hex)
            && !colours.contains(&rgb.hex())
        {
            colours.push(rgb.hex());
        }
    };
    for key in [
        "background",
        "surface",
        "surfaceVariant",
        "surfaceContainer",
        "outline",
        "primary",
        "tertiary",
        "primaryText",
        "surfaceText",
        "on_primary",
        "on_surface",
    ] {
        add(&theme[key]);
    }
    let variant =
        if theme["_scheme"]["is_dark_mode"].as_bool().unwrap_or(true) { "dark" } else { "light" };
    if let Some(roles) = theme["_scheme"]["colors"].as_object() {
        for (role, value) in roles {
            if role != "source_color" {
                add(&value[variant]["color"]);
            }
        }
    }
    colours
}

pub(super) fn extend(list: &mut Value, config: &Config) {
    let options: Vec<Value> = config
        .theme()
        .saved_themes()
        .iter()
        .filter_map(|theme| {
            let name = theme["name"].as_str().filter(|name| !name.trim().is_empty())?;
            let colours = colours(theme);
            if colours.is_empty() {
                return None;
            }
            Some(json!({"mode": format!("saved:{name}"), "label": name, "swatch": colours}))
        })
        .collect();
    for effect in list.as_array_mut().into_iter().flatten() {
        if !matches!(effect["id"].as_str(), Some("theme" | "gradientmap")) {
            continue;
        }
        for param in effect["params"].as_array_mut().into_iter().flatten() {
            if param["id"] == "theme"
                && let Some(choices) = param["options"].as_array_mut()
            {
                choices.extend(options.iter().cloned());
            }
        }
    }
}

pub(crate) fn resolve(effects: &mut Value, config: &Config) -> anyhow::Result<()> {
    for step in effects.as_array_mut().into_iter().flatten() {
        if !matches!(step["effect"].as_str(), Some("theme" | "gradientmap")) {
            continue;
        }
        let Some(name) = step["params"]["theme"].as_str().and_then(|id| id.strip_prefix("saved:"))
        else {
            continue;
        };
        let saved = config.theme().saved_themes();
        let theme = saved
            .iter()
            .find(|theme| theme["name"].as_str() == Some(name))
            .ok_or_else(|| anyhow::anyhow!("saved theme {name:?} no longer exists"))?;
        let palette = colours(theme);
        if palette.is_empty() {
            anyhow::bail!("saved theme {name:?} has no valid colours");
        }
        step["params"]["palette"] = json!(palette);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
