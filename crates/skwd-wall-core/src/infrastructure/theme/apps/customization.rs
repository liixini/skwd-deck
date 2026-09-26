use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use wall_proto::AppThemeStatus;

use super::{catalogue, files, manager::Environment, structured, waybar};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Mapping {
    pub path: Vec<String>,
    pub role: String,
}

fn path(env: &Environment, id: &str) -> PathBuf {
    env.config.join("skwd-wall-v2/app-themes").join(format!("{id}.template"))
}

fn marker(env: &Environment, id: &str) -> PathBuf {
    env.receipts.join(format!("{id}-disconnected.json"))
}

pub(super) fn disconnected(env: &Environment, id: &str) -> bool {
    marker(env, id).exists()
}

pub(super) fn describe(env: &Environment, status: &mut AppThemeStatus) {
    status.template_path = path(env, &status.id).display().to_string();
    status.customized = path(env, &status.id).exists();
    status.can_disconnect = receipt_paths(env, &status.id).iter().any(|path| path.exists());
    status.can_reconnect = status.installed && status.can_disconnect;
    if disconnected(env, &status.id) {
        status.state = "disconnected".into();
        status.enabled = false;
        status.can_enable = false;
        status.can_disable = false;
        status.can_disconnect = false;
        status.detail.clear();
    }
}

pub(super) fn template(env: &Environment, id: &str, default: &str) -> Result<String> {
    Ok(files::read(&path(env, id))?.unwrap_or_else(|| default.into()))
}

pub(super) fn defaults(id: &str) -> Result<String> {
    if id == "waybar" {
        return Ok(waybar::TEMPLATE.into());
    }
    if let Some(recipe) =
        catalogue::RECIPES.iter().chain([&catalogue::KDE_RECIPE]).find(|r| r.id == id)
    {
        return Ok(recipe.template.into());
    }
    ensure!(structured::APPS.iter().any(|app| app.id == id), "Unknown app theme");
    Ok(serde_json::to_string_pretty(
        &structured::roles(id)
            .into_iter()
            .map(|(path, role)| Mapping {
                path: path.into_iter().map(str::to_owned).collect(),
                role: role.into(),
            })
            .collect::<Vec<_>>(),
    )? + "\n")
}

pub(super) fn mappings(env: &Environment, id: &str) -> Result<Vec<Mapping>> {
    let mappings: Vec<Mapping> = serde_json::from_str(&template(env, id, &defaults(id)?)?)
        .context("The field mappings must be a JSON list of path and role entries")?;
    let mut seen = std::collections::HashSet::new();
    for mapping in &mappings {
        ensure!(
            crate::material::ROLE_KEYS.contains(&mapping.role.as_str())
                || crate::static_templates::material_map(&Value::Null, true)
                    .contains_key(mapping.role.as_str()),
            "Unknown palette role: {}",
            mapping.role
        );
        ensure!(
            !mapping.path.is_empty() && mapping.path.iter().all(|part| !part.is_empty()),
            "A mapping needs a non-empty field path"
        );
        let allowed = match id {
            "code" => mapping.path.len() == 2 && mapping.path[0] == "workbench.colorCustomizations",
            "alacritty" => mapping.path.len() >= 3 && mapping.path[0] == "colors",
            "yazi" => {
                mapping.path.len() >= 2
                    && matches!(mapping.path.last().map(String::as_str), Some("fg" | "bg"))
            }
            _ => false,
        };
        ensure!(allowed, "The mapping path must name a colour field for {id}");
        ensure!(seen.insert(mapping.path.clone()), "Duplicate colour field mapping");
    }
    Ok(mappings)
}

pub(super) fn receipt_paths(env: &Environment, id: &str) -> Vec<PathBuf> {
    let names = if structured::APPS.iter().any(|app| app.id == id) {
        vec![format!("{id}-settings.json")]
    } else if id == "waybar" {
        vec!["waybar-overlay.json".into(), "waybar.json".into()]
    } else {
        vec![format!("{id}.json")]
    };
    names.into_iter().map(|name| env.receipts.join(name)).collect()
}

pub(super) fn backup(env: &Environment, id: &str) -> Result<PathBuf> {
    let mut paths = vec![path(env, id)];
    for receipt in receipt_paths(env, id) {
        if let Some(text) = files::read(&receipt)? {
            let value: Value = serde_json::from_str(&text)?;
            for key in ["config", "output"] {
                if let Some(path) = value[key].as_str() {
                    paths.push(path.into());
                }
            }
            if id == "kde" {
                for name in ["SkwdManaged", "SkwdManagedAlt"] {
                    paths.push(env.data.join(format!("color-schemes/{name}.colors")));
                }
            }
            if let Some(styles) = value["styles"].as_array() {
                paths.extend(
                    styles.iter().filter_map(|style| style["path"].as_str().map(PathBuf::from)),
                );
            }
            paths.push(receipt);
        }
    }
    paths.sort();
    paths.dedup();
    let snapshots = paths
        .into_iter()
        .map(|path| Ok(json!({"path": path, "text": files::read(&path)?})))
        .collect::<Result<Vec<_>>>()?;
    let directory = env.receipts.join("backups");
    std::fs::create_dir_all(&directory)?;
    let mut file = tempfile::Builder::new()
        .prefix(&format!("{id}-"))
        .suffix(".json")
        .tempfile_in(&directory)?;
    file.write_all(serde_json::to_string_pretty(&snapshots)?.as_bytes())?;
    file.as_file().sync_all()?;
    Ok(file.keep()?.1)
}

pub(super) fn disconnect(env: &Environment, id: &str) -> Result<()> {
    ensure!(
        receipt_paths(env, id).iter().any(|path| path.exists()),
        "This app has no saved theme setup"
    );
    files::write(&marker(env, id), "{}\n")
}

pub(super) fn connected(env: &Environment, id: &str) -> Result<()> {
    let path = marker(env, id);
    if path.exists() {
        files::writable(&path)?;
        std::fs::remove_file(path)?;
    }
    Ok(())
}

pub(super) fn edit_template(env: &Environment, id: &str, reset: bool) -> Result<()> {
    let path = path(env, id);
    if reset && path.exists() {
        backup(env, id)?;
        files::writable(&path)?;
        std::fs::remove_file(path)?;
    } else if !reset && !path.exists() {
        files::write(&path, &defaults(id)?)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
