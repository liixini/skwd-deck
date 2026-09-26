use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use wall_proto::AppThemeStatus;

use super::{catalogue::RECIPES, files, manager, manager::Environment};
use crate::config::Config;

mod paths;

const MARKER: &str = "/* Skwd Waybar colours */";
pub(super) const TEMPLATE: &str = concat!(
    include_str!("../../../../../../data/matugen/templates/waybar.css"),
    include_str!("../../../../../../data/app-themes/waybar.css"),
);

#[derive(Deserialize, Serialize)]
struct Style {
    path: PathBuf,
    original: Option<String>,
    base: String,
}

#[derive(Deserialize, Serialize)]
struct Receipt {
    version: u32,
    enabled: bool,
    pending: bool,
    output: PathBuf,
    rendered: String,
    styles: Vec<Style>,
    result: String,
}

fn receipt_path(env: &Environment) -> PathBuf {
    env.receipts.join("waybar-overlay.json")
}

fn output_path(env: &Environment) -> PathBuf {
    env.config.join("waybar/skwd-theme.css")
}

fn load(env: &Environment) -> Result<Option<Receipt>> {
    let Some(text) = files::read(&receipt_path(env))? else { return Ok(None) };
    let receipt: Receipt = serde_json::from_str(&text).context("Cannot read saved Waybar setup")?;
    ensure!(
        receipt.version == 1 && receipt.output == output_path(env),
        "Saved Waybar setup location changed"
    );
    Ok(Some(receipt))
}

fn save(env: &Environment, receipt: &Receipt) -> Result<()> {
    files::write(&receipt_path(env), &serde_json::to_string_pretty(receipt)?)
}

fn directive(output: &Path) -> String {
    format!("@import \"{}\";", paths::css_path(output))
}

fn import_line(line: &str, output: &Path) -> bool {
    let Some(value) =
        line.trim().strip_prefix("@import").and_then(|text| text.trim().strip_suffix(';'))
    else {
        return false;
    };
    let value = value.trim();
    let value =
        value.strip_prefix("url(").and_then(|text| text.strip_suffix(')')).unwrap_or(value).trim();
    let path = paths::css_path(output);
    value == format!("\"{path}\"") || value == format!("'{path}'")
}

fn strip(text: &str, output: &Path) -> String {
    let import = directive(output);
    let block = format!("\n{MARKER}\n{import}\n");
    if text.matches(&block).count() == 1 {
        return text.replacen(&block, "", 1);
    }
    text.split_inclusive('\n')
        .filter(|line| {
            let line = line.trim();
            line != MARKER && !import_line(line, output)
        })
        .collect()
}

fn connected(text: &str, output: &Path) -> bool {
    text.lines().any(|line| import_line(line, output))
}

fn conflict(config: &Config, output: &Path, styles: &[Style]) -> Option<String> {
    config.theme().integrations().iter().find_map(|entry| {
        let path = crate::static_templates::integration_output(config, &entry.output);
        (paths::same(&path, output)
            || styles.iter().any(|style| paths::same(&path, &style.path)))
        .then(|| {
            format!(
                "Custom output '{}' also controls Waybar; turn it off before enabling the overlay",
                entry.name
            )
        })
    })
}

pub(super) fn inspect(env: &Environment, config: &Config) -> AppThemeStatus {
    let installed = env.executable("waybar").is_some();
    let mut status = AppThemeStatus {
        id: "waybar".into(),
        name: "Waybar".into(),
        installed,
        enabled: false,
        config_found: false,
        config_path: env.config.join("waybar/style.css").display().to_string(),
        output_path: output_path(env).display().to_string(),
        state: "ready".into(),
        detail: String::new(),
        can_enable: installed,
        can_disable: false,
        can_adopt: false,
        template_path: String::new(),
        customized: false,
        can_disconnect: false,
        can_reconnect: false,
    };
    let check = || -> Result<(Vec<Style>, Option<Receipt>)> {
        let saved = load(env)?;
        if let Some(receipt) = saved.filter(|receipt| receipt.enabled || receipt.pending) {
            return Ok((Vec::new(), Some(receipt)));
        }
        files::writable(&output_path(env))?;
        ensure!(
            !output_path(env).exists(),
            "A file already exists at {}; review it before enabling Waybar colours",
            output_path(env).display()
        );
        Ok((paths::discover(env)?, None))
    };
    match check() {
        Ok((styles, saved)) => {
            let styles =
                saved.as_ref().map_or(styles.as_slice(), |receipt| receipt.styles.as_slice());
            status.config_path = styles
                .iter()
                .map(|style| style.path.display().to_string())
                .collect::<Vec<_>>()
                .join("\n");
            status.config_found = styles.iter().all(|style| style.path.exists());
            if let Some(receipt) = &saved {
                status.enabled = receipt.enabled;
                status.can_disable = true;
                status.state.clone_from(&receipt.result);
                if receipt.pending {
                    status.state = "interrupted".into();
                } else if styles.iter().any(|style| {
                    !files::read(&style.path)
                        .ok()
                        .flatten()
                        .is_some_and(|text| connected(&text, &receipt.output))
                }) {
                    status.state = "changed".into();
                    status.detail =
                        "The Waybar overlay import is missing; switch it off and on to reconnect"
                            .into();
                } else if files::read(&receipt.output).ok().flatten().as_deref()
                    != Some(&receipt.rendered)
                {
                    status.state = "changed".into();
                    status.detail = format!(
                        "{} changed or was removed; switch Waybar colours off before reviewing the file",
                        receipt.output.display()
                    );
                }
            } else if let Ok(Some(legacy)) = files::load(&env.receipts.join("waybar.json"))
                && (legacy.enabled || legacy.pending)
            {
                status.enabled = legacy.enabled;
                status.can_disable = true;
                status.state = "configured".into();
                status.output_path = legacy.output.display().to_string();
                let restored = files::read(&legacy.config)
                    .and_then(|text| restore_legacy(&legacy, text.as_deref().unwrap_or_default()));
                if let Err(error) = restored {
                    status.state = "needs-review".into();
                    status.detail = error.to_string();
                    status.can_disable = false;
                }
            }
            if let Some(reason) = conflict(config, &output_path(env), styles) {
                status.state = "conflict".into();
                status.detail = "custom-output".into();
                status.can_adopt = saved.is_none() && manager::migratable(config, recipe());
                log::debug!("{reason}");
            }
        }
        Err(error) => {
            status.state = "needs-review".into();
            status.detail = error.to_string();
        }
    }
    if !installed && status.state == "ready" {
        status.state = "not-installed".into();
    }
    status.can_enable &= status.state == "ready";
    status
}

fn recipe() -> &'static super::catalogue::Recipe {
    RECIPES.iter().find(|recipe| recipe.id == "waybar").unwrap()
}

fn rendered(env: &Environment, palette: &Value, dark: bool) -> Result<String> {
    ensure!(
        super::super::profiles::valid_palette(palette),
        "Apply a wallpaper or choose a colour theme first"
    );
    let text = crate::static_templates::render_palette(
        &super::customization::template(env, "waybar", TEMPLATE)?,
        palette,
        dark,
    );
    ensure!(!text.contains("{{"), "Waybar colours contain unsupported palette values");
    Ok(text)
}

fn restore_legacy(receipt: &files::Receipt, text: &str) -> Result<String> {
    let text = files::restore(receipt, recipe(), text).unwrap_or_else(|_| text.to_owned());
    let name = receipt.output.file_name().context("Saved Waybar colours have no file name")?;
    let relative = Path::new(name);
    let dotted = Path::new(".").join(relative);
    let restored: String = text
        .split_inclusive('\n')
        .filter(|line| {
            let marker = line.split_ascii_whitespace().collect::<String>();
            !matches!(marker.as_str(), "/*Skwdapptheme*/" | "/*EndSkwdapptheme*/")
                && ![receipt.output.as_path(), relative, dotted.as_path()]
                    .iter()
                    .any(|output| import_line(line, output))
        })
        .collect();
    ensure!(
        !restored.contains(name.to_string_lossy().as_ref()),
        "The legacy Waybar import in {} needs review; put its skwd-colors.css import on a separate line before retrying",
        receipt.config.display()
    );
    Ok(restored)
}

fn retire_legacy(env: &Environment) -> Result<()> {
    let path = env.receipts.join("waybar.json");
    if let Some(mut receipt) =
        files::load(&path)?.filter(|receipt| receipt.enabled || receipt.pending)
    {
        if let Some(text) = files::read(&receipt.config)? {
            let restored = restore_legacy(&receipt, &text)?;
            if restored != text {
                files::write(&receipt.config, &restored)?;
            }
        }
        if files::read(&receipt.output)?.as_deref() == Some(&receipt.rendered) {
            std::fs::remove_file(&receipt.output)?;
        }
        receipt.enabled = false;
        receipt.pending = false;
        files::save(&path, &receipt)?;
    }
    Ok(())
}

pub(super) fn set(
    env: &Environment,
    config: &Config,
    enabled: bool,
    palette: &Value,
    dark: bool,
) -> Result<()> {
    let saved = load(env)?;
    if !enabled {
        if let Some(mut receipt) = saved.filter(|receipt| receipt.enabled || receipt.pending) {
            for style in &receipt.styles {
                if let Some(text) = files::read(&style.path)? {
                    let restored = strip(&text, &receipt.output);
                    if style.original.is_none() && restored == style.base {
                        files::writable(&style.path)?;
                        std::fs::remove_file(&style.path)?;
                    } else if restored != text {
                        files::write(&style.path, &restored)?;
                    }
                }
            }
            if files::read(&receipt.output)?.as_deref() == Some(&receipt.rendered) {
                files::writable(&receipt.output)?;
                std::fs::remove_file(&receipt.output)?;
            }
            receipt.enabled = false;
            receipt.pending = false;
            receipt.result = super::reload::reload(env, recipe());
            save(env, &receipt)?;
        }
        return retire_legacy(env);
    }
    ensure!(env.executable("waybar").is_some(), "Waybar is not installed");
    let text = rendered(env, palette, dark)?;
    if let Some(receipt) = saved.filter(|receipt| receipt.enabled || receipt.pending) {
        ensure!(
            !receipt.pending,
            "Waybar setup was interrupted; switch it off before trying again"
        );
        return publish(env, config, receipt, &text, true);
    }
    let styles = paths::discover(env)?;
    ensure!(
        conflict(config, &output_path(env), &styles).is_none(),
        "{}",
        conflict(config, &output_path(env), &styles).unwrap_or_default()
    );
    ensure!(!output_path(env).exists(), "A file already exists at {}", output_path(env).display());
    retire_legacy(env)?;
    let styles = paths::discover(env)?;
    let mut receipt = Receipt {
        version: 1,
        enabled: false,
        pending: true,
        output: output_path(env),
        rendered: text,
        styles,
        result: "configured".into(),
    };
    files::writable(&receipt.output)?;
    save(env, &receipt)?;
    files::write(&receipt.output, &receipt.rendered)?;
    for style in &receipt.styles {
        ensure!(
            files::read(&style.path)? == style.original,
            "{} changed during setup; switch Waybar colours off and retry",
            style.path.display()
        );
        let next = format!("{}\n{MARKER}\n{}\n", style.base, directive(&receipt.output));
        files::write(&style.path, &next)?;
    }
    receipt.enabled = true;
    receipt.pending = false;
    receipt.result = super::reload::reload(env, recipe());
    save(env, &receipt)
}

fn publish(
    env: &Environment,
    config: &Config,
    mut receipt: Receipt,
    text: &str,
    force_reload: bool,
) -> Result<()> {
    if let Some(reason) = conflict(config, &receipt.output, &receipt.styles) {
        anyhow::bail!("{reason}");
    }
    for style in &receipt.styles {
        ensure!(
            files::read(&style.path)?.is_some_and(|text| connected(&text, &receipt.output)),
            "The overlay import in {} was removed; switch Waybar colours off and on to reconnect",
            style.path.display()
        );
    }
    ensure!(
        files::read(&receipt.output)?.as_deref() == Some(&receipt.rendered),
        "{} changed or was removed; switch Waybar colours off before reviewing the file",
        receipt.output.display()
    );
    if text != receipt.rendered {
        files::write(&receipt.output, text)?;
        receipt.rendered = text.into();
    } else if !force_reload && receipt.result != "reload-needed" {
        return Ok(());
    }
    receipt.result = super::reload::reload(env, recipe());
    save(env, &receipt)
}

pub(super) fn update(
    env: &Environment,
    config: &Config,
    palette: &Value,
    dark: bool,
) -> Result<()> {
    if let Some(receipt) = load(env)?.filter(|receipt| receipt.enabled && !receipt.pending) {
        publish(env, config, receipt, &rendered(env, palette, dark)?, false)?;
    }
    Ok(())
}

pub(crate) fn protects_output(output: &Path) -> bool {
    protects(&Environment::current(), output)
}

pub(super) fn managed(env: &Environment) -> bool {
    load(env).ok().flatten().is_some_and(|receipt| receipt.enabled || receipt.pending)
}

fn protects(env: &Environment, output: &Path) -> bool {
    load(env).ok().flatten().filter(|receipt| receipt.enabled || receipt.pending).is_some_and(
        |receipt| {
            paths::same(output, &receipt.output)
                || receipt.styles.iter().any(|style| paths::same(output, &style.path))
        },
    )
}

#[cfg(test)]
mod tests;

pub(super) fn reconnect(
    env: &Environment,
    config: &Config,
    palette: &Value,
    dark: bool,
) -> Result<()> {
    let text = rendered(env, palette, dark)?;
    let Some(mut receipt) = load(env)? else {
        retire_legacy(env)?;
        if output_path(env).exists() {
            files::writable(&output_path(env))?;
            std::fs::remove_file(output_path(env))?;
        }
        return set(env, config, true, palette, dark);
    };
    ensure!(
        conflict(config, &receipt.output, &receipt.styles).is_none(),
        "Another custom output controls Waybar; turn it off before reconnecting"
    );
    for style in &mut receipt.styles {
        let current = files::read(&style.path)?;
        style.base = strip(current.as_deref().unwrap_or_default(), &receipt.output);
        style.original = current.map(|_| style.base.clone());
    }
    receipt.pending = true;
    save(env, &receipt)?;
    files::write(&receipt.output, &text)?;
    for style in &receipt.styles {
        files::write(
            &style.path,
            &format!("{}\n{MARKER}\n{}\n", style.base, directive(&receipt.output)),
        )?;
    }
    receipt.rendered = text;
    receipt.enabled = true;
    receipt.pending = false;
    receipt.result = super::reload::reload(env, recipe());
    save(env, &receipt)
}
