use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use wall_proto::AppThemeStatus;

use super::{documents::Document, files, manager::Environment};

pub(super) struct App {
    pub id: &'static str,
    name: &'static str,
    config: &'static str,
    json: bool,
}

pub(super) const APPS: [App; 3] = [
    App { id: "code", name: "Visual Studio Code", config: "Code/User/settings.json", json: true },
    App { id: "alacritty", name: "Alacritty", config: "alacritty/alacritty.toml", json: false },
    App { id: "yazi", name: "Yazi", config: "yazi/theme.toml", json: false },
];

#[derive(Clone, Deserialize, Serialize)]
struct Edit {
    path: Vec<String>,
    before: Option<String>,
    after: String,
}

#[derive(Deserialize, Serialize)]
struct Receipt {
    version: u32,
    config: PathBuf,
    original: Option<String>,
    published: String,
    enabled: bool,
    pending: bool,
    #[serde(default)]
    pending_before: Option<String>,
    edits: Vec<Edit>,
}

impl App {
    pub(super) fn matches(&self, name: &str) -> bool {
        name.eq_ignore_ascii_case(self.id)
            || self.id == "code" && name.eq_ignore_ascii_case("vscode")
    }

    pub(super) fn migratable(&self, config: &crate::config::Config) -> bool {
        let matching: Vec<_> = config
            .theme()
            .integrations()
            .into_iter()
            .filter(|entry| self.matches(&entry.name))
            .collect();
        let expected = match self.id {
            "code" => "vscode-theme.json",
            "yazi" => "yazi-theme.toml",
            _ => return false,
        };
        !matching.is_empty()
            && matching.iter().all(|entry| {
                std::path::Path::new(&entry.template)
                    .file_name()
                    .is_some_and(|name| name == expected)
            })
    }

    fn paths(&self, env: &Environment) -> (PathBuf, PathBuf) {
        let mut config = env.config.join(self.config);
        if self.id == "alacritty" && !config.exists() && env.config.join("alacritty.toml").exists()
        {
            config = env.config.join("alacritty.toml");
        }
        if self.id == "alacritty" && !config.exists() && env.home.join(".alacritty.toml").exists() {
            config = env.home.join(".alacritty.toml");
        }
        if self.id == "code"
            && !config.exists()
            && env.config.join("Code - OSS/User/settings.json").exists()
        {
            config = env.config.join("Code - OSS/User/settings.json");
        }
        (config, env.receipts.join(format!("{}-settings.json", self.id)))
    }

    fn load(&self, env: &Environment) -> Result<Option<Receipt>> {
        let (path, receipt) = self.paths(env);
        let Some(text) = files::read(&receipt)? else { return Ok(None) };
        let value: Receipt = serde_json::from_str(&text)?;
        ensure!(
            value.version == 1 && value.config == path,
            "Saved theme location changed; review its setup"
        );
        Ok(Some(value))
    }

    fn status(&self) -> &'static str {
        if self.id == "yazi" { "on-next-open" } else { "watching" }
    }

    pub(super) fn inspect(
        &self,
        env: &Environment,
        config: &crate::config::Config,
    ) -> AppThemeStatus {
        let (path, _) = self.paths(env);
        let installed = env.executable(self.id).is_some();
        let mut status = AppThemeStatus {
            id: self.id.into(),
            name: self.name.into(),
            installed,
            config_found: path.exists(),
            config_path: path.display().to_string(),
            output_path: path.display().to_string(),
            enabled: false,
            state: if installed { "ready" } else { "not-installed" }.into(),
            detail: String::new(),
            can_enable: installed,
            can_disable: false,
            can_adopt: false,
        };
        let check = || -> Result<(bool, bool, bool)> {
            files::writable(&path)?;
            if self.id == "alacritty"
                && !path.exists()
                && std::path::Path::new("/etc/alacritty/alacritty.toml").exists()
            {
                anyhow::bail!(
                    "Alacritty uses system settings; create a user config before enabling colours"
                );
            }
            let text = files::read(&path)?.unwrap_or_default();
            let document = Document::parse(&text, self.json)?;
            if let Some(receipt) = self.load(env)?.filter(|r| r.enabled || r.pending) {
                let valid = owns(&document, &receipt.edits)?
                    || receipt.pending
                        && (text == receipt.pending_before.as_deref().unwrap_or_default()
                            || text == receipt.published);
                return Ok((receipt.enabled, receipt.pending, valid));
            }
            ensure!(
                !config.theme().integrations().iter().any(|entry| self.matches(&entry.name)),
                "custom-output"
            );
            ensure!(
                (config.theme().disabled_integration(self.id)
                    || self.id == "code" && config.theme().disabled_integration("vscode")
                    || !text.to_ascii_lowercase().contains("matugen"))
                    && !text.to_ascii_lowercase().contains("noctalia"),
                "Another theme setup needs review"
            );
            if self.id == "alacritty" {
                ensure!(
                    document
                        .get(&["general".into(), "live_config_reload".into()])?
                        .as_deref()
                        .map(str::trim)
                        != Some("false"),
                    "Alacritty live config reload is disabled"
                );
            }
            Ok((false, false, true))
        };
        match check() {
            Ok((enabled, pending, valid)) => {
                status.enabled = enabled;
                status.can_disable = (enabled || pending) && valid;
                status.can_enable &= !enabled && !pending && valid;
                if pending {
                    status.state = "interrupted".into();
                } else if !valid {
                    status.state = "changed".into();
                } else if enabled {
                    status.state = self.status().into();
                }
            }
            Err(error) => {
                status.detail = error.to_string();
                status.state =
                    if status.detail == "custom-output" { "conflict" } else { "needs-review" }
                        .into();
                status.can_adopt = status.detail == "custom-output" && self.migratable(config);
                status.can_enable = false;
            }
        }
        status
    }

    pub(super) fn set(
        &self,
        env: &Environment,
        config: &crate::config::Config,
        enabled: bool,
        palette: &Value,
        dark: bool,
    ) -> Result<()> {
        let (path, saved) = self.paths(env);
        let current = files::read(&path)?;
        let text = current.as_deref().unwrap_or_default();
        let mut document = Document::parse(text, self.json)?;
        let receipt = self.load(env)?.filter(|r| r.enabled || r.pending);
        if !enabled {
            let Some(mut receipt) = receipt else { return Ok(()) };
            let restored = if text == receipt.published
                || receipt.pending && current == receipt.pending_before
            {
                receipt.original.clone()
            } else {
                ensure!(
                    owns(&document, &receipt.edits)?,
                    "Theme colours changed outside Skwd; review the settings before restoring"
                );
                for edit in &receipt.edits {
                    document.set(&edit.path, edit.before.as_deref())?;
                }
                Some(document.text())
            };
            ensure!(
                files::read(&path)? == current,
                "Settings changed during restoration; try again"
            );
            receipt.original.clone_from(&restored);
            receipt.published = restored.clone().unwrap_or_default();
            receipt.pending_before.clone_from(&current);
            receipt.pending = true;
            files::write(&saved, &serde_json::to_string_pretty(&receipt)?)?;
            if let Some(restored) = restored {
                files::write(&path, &restored)?;
            } else if path.exists() {
                files::writable(&path)?;
                std::fs::remove_file(&path)?;
            }
            receipt.enabled = false;
            receipt.pending = false;
            return files::write(&saved, &serde_json::to_string_pretty(&receipt)?);
        }
        ensure!(
            super::super::profiles::valid_palette(palette),
            "Apply a wallpaper or choose a colour theme first"
        );
        let mut receipt = if let Some(mut receipt) = receipt {
            ensure!(
                !receipt.pending && owns(&document, &receipt.edits)?,
                "Theme colours changed outside Skwd; review the settings"
            );
            if text != receipt.published {
                let mut restored = Document::parse(text, self.json)?;
                for edit in &receipt.edits {
                    restored.set(&edit.path, edit.before.as_deref())?;
                }
                receipt.original = Some(restored.text());
            }
            receipt
        } else {
            let status = self.inspect(env, config);
            ensure!(status.can_enable, "Theme setup is unavailable: {}", status.detail);
            Receipt {
                version: 1,
                config: path.clone(),
                original: current.clone(),
                published: String::new(),
                enabled: false,
                pending: false,
                pending_before: None,
                edits: Vec::new(),
            }
        };
        let mut edits = Vec::new();
        for (keys, role) in roles(self.id) {
            let path: Vec<String> = keys.into_iter().map(str::to_owned).collect();
            let before = match receipt.edits.iter().find(|edit| edit.path == path) {
                Some(edit) => edit.before.clone(),
                None => document.get(&path)?,
            };
            let color = crate::static_templates::render_palette(
                &format!("{{{{colors.{role}.default.hex}}}}"),
                palette,
                dark,
            );
            ensure!(!color.contains("{{"), "Missing palette colour");
            let after = serde_json::to_string(&color)?;
            document.set(&path, Some(&after))?;
            edits.push(Edit { path, before, after });
        }
        let next = document.text();
        receipt.edits = edits;
        receipt.published.clone_from(&next);
        receipt.pending_before.clone_from(&current);
        receipt.pending = true;
        files::write(&saved, &serde_json::to_string_pretty(&receipt)?)?;
        ensure!(files::read(&path)? == current, "Settings changed during setup; try again");
        if text != next {
            files::write(&path, &next)?;
        }
        receipt.enabled = true;
        receipt.pending = false;
        files::write(&saved, &serde_json::to_string_pretty(&receipt)?)
    }
}

fn owns(document: &Document, edits: &[Edit]) -> Result<bool> {
    for edit in edits {
        if document.get(&edit.path)?.as_deref().map(str::trim) != Some(edit.after.trim()) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn roles(id: &str) -> Vec<(Vec<&'static str>, &'static str)> {
    match id {
        "code" => [
            ("editor.background", "surface"),
            ("editor.foreground", "on_surface"),
            ("sideBar.background", "surface_container"),
            ("sideBar.foreground", "on_surface"),
            ("activityBar.background", "surface_container_low"),
            ("activityBar.foreground", "primary"),
            ("statusBar.background", "primary"),
            ("statusBar.foreground", "on_primary"),
            ("editorCursor.foreground", "primary"),
            ("focusBorder", "primary"),
        ]
        .into_iter()
        .map(|(key, role)| (vec!["workbench.colorCustomizations", key], role))
        .collect(),
        "alacritty" => vec![
            (vec!["colors", "primary", "background"], "surface"),
            (vec!["colors", "primary", "foreground"], "on_surface"),
            (vec!["colors", "cursor", "cursor"], "primary"),
            (vec!["colors", "cursor", "text"], "on_primary"),
            (vec!["colors", "selection", "background"], "primary"),
            (vec!["colors", "selection", "text"], "on_primary"),
        ],
        _ => vec![
            (vec!["mgr", "cwd", "fg"], "primary"),
            (vec!["mode", "normal_main", "bg"], "primary"),
            (vec!["mode", "normal_main", "fg"], "on_primary"),
            (vec!["mode", "select_main", "bg"], "secondary"),
            (vec!["mode", "select_main", "fg"], "on_secondary"),
            (vec!["mode", "unset_main", "bg"], "tertiary"),
            (vec!["mode", "unset_main", "fg"], "on_tertiary"),
        ],
    }
}

#[cfg(test)]
#[path = "structured_tests.rs"]
mod tests;
