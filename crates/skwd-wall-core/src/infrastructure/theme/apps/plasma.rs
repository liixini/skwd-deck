use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use wall_proto::AppThemeStatus;

use super::catalogue::KDE_RECIPE;
use super::files;
use super::manager::{self, Environment};

const NAMES: [&str; 2] = ["SkwdManaged", "SkwdManagedAlt"];

#[derive(Deserialize, Serialize)]
struct Receipt {
    version: u32,
    config: PathBuf,
    directory: PathBuf,
    previous: String,
    active: String,
    outputs: BTreeMap<String, String>,
    enabled: bool,
    pending: bool,
}

fn selected(env: &Environment) -> Result<String> {
    let text = files::read(&env.config.join("kdeglobals"))?.unwrap_or_default();
    let mut general = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            general = line == "[General]";
        } else if general
            && let Some((key, value)) = line.split_once('=')
            && key.trim() == "ColorScheme"
        {
            return Ok(value.trim().to_string());
        }
    }
    Ok("BreezeLight".into())
}

fn scheme_exists(env: &Environment, name: &str) -> bool {
    !name.is_empty()
        && !name.contains(['/', '\\'])
        && std::iter::once(&env.data)
            .chain(env.data_dirs.iter())
            .any(|dir| dir.join("color-schemes").join(format!("{name}.colors")).is_file())
}

fn receipt_path(env: &Environment) -> PathBuf {
    env.receipts.join("kde.json")
}

fn load(env: &Environment) -> Result<Option<Receipt>> {
    let Some(text) = files::read(&receipt_path(env))? else { return Ok(None) };
    let value: Receipt = serde_json::from_str(&text)?;
    ensure!(
        value.version == 1
            && value.config == env.config.join("kdeglobals")
            && value.directory == env.data.join("color-schemes"),
        "The saved Plasma theme location changed; review its setup"
    );
    ensure!(
        value.outputs.keys().all(|name| NAMES.contains(&name.as_str())),
        "Unknown managed Plasma scheme"
    );
    Ok(Some(value))
}

fn save(env: &Environment, receipt: &Receipt) -> Result<()> {
    files::write(&receipt_path(env), &serde_json::to_string_pretty(receipt)?)
}

fn owned(receipt: &Receipt, current: &str) -> bool {
    current == receipt.active
        || receipt.pending && (current == receipt.previous || receipt.outputs.contains_key(current))
}

pub(super) fn inspect(env: &Environment, config: &crate::config::Config) -> AppThemeStatus {
    let path = env.config.join("kdeglobals");
    let output = env.data.join("color-schemes/SkwdManaged.colors");
    let installed = env.executable("plasma-apply-colorscheme").is_some();
    let mut status = AppThemeStatus {
        id: "kde".into(),
        name: "KDE Plasma".into(),
        installed,
        config_found: path.exists(),
        config_path: path.display().to_string(),
        output_path: output.display().to_string(),
        enabled: false,
        state: if installed { "ready" } else { "not-installed" }.into(),
        detail: String::new(),
        can_enable: installed,
        can_disable: false,
        can_adopt: false,
    };
    let check = || -> Result<Option<Receipt>> {
        files::writable(&path)?;
        for name in NAMES {
            files::writable(&env.data.join(format!("color-schemes/{name}.colors")))?;
        }
        load(env)
    };
    let check = (|| -> Result<()> {
        if let Some(receipt) = check()?.filter(|r| r.enabled || r.pending) {
            let current = selected(env)?;
            status.enabled = receipt.enabled;
            status.can_enable = false;
            status.can_disable =
                installed && owned(&receipt, &current) && scheme_exists(env, &receipt.previous);
            status.state = if receipt.pending {
                "interrupted"
            } else if !owned(&receipt, &current)
                || receipt.outputs.iter().any(|(name, text)| {
                    files::read(&receipt.directory.join(format!("{name}.colors")))
                        .ok()
                        .flatten()
                        .as_ref()
                        != Some(text)
                })
            {
                "changed"
            } else {
                "applied"
            }
            .into();
            status.output_path =
                receipt.directory.join(format!("{}.colors", receipt.active)).display().to_string();
        } else if let Some(owner) =
            manager::conflict(config, &KDE_RECIPE, &files::read(&path)?.unwrap_or_default())
        {
            status.state = "conflict".into();
            status.detail = owner;
            status.can_enable = false;
            status.can_adopt =
                status.detail == "custom-output" && manager::migratable(config, &KDE_RECIPE);
        } else if installed {
            ensure!(
                scheme_exists(env, &selected(env)?),
                "The previous Plasma colour scheme cannot be found; select an installed scheme first"
            );
            for name in NAMES {
                ensure!(
                    !env.data.join(format!("color-schemes/{name}.colors")).exists(),
                    "A managed Plasma scheme already exists; review its setup"
                );
            }
        }
        Ok(())
    })();
    if let Err(error) = check {
        status.state = "needs-review".into();
        status.detail = error.to_string();
        status.can_enable = false;
    }
    status
}

fn apply_scheme(env: &Environment, name: &str) -> Result<()> {
    let program = env
        .executable("plasma-apply-colorscheme")
        .context("Plasma colour tools are not installed")?;
    let mut child = crate::proc::tool(program)
        .arg(name)
        .env("XDG_CONFIG_HOME", &env.config)
        .env("XDG_DATA_HOME", &env.data)
        .env("XDG_DATA_DIRS", std::env::join_paths(&env.data_dirs)?)
        .env("QT_QPA_PLATFORM", "offscreen")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Some(status) = child.try_wait()? {
            ensure!(status.success(), "Plasma could not apply the colour scheme");
            ensure!(selected(env)? == name, "Plasma did not select the requested colour scheme");
            return Ok(());
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("Plasma took too long to refresh its colours");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn output(receipt: &Receipt, name: &str) -> PathBuf {
    receipt.directory.join(format!("{name}.colors"))
}

fn remove_outputs(receipt: &Receipt) -> Result<()> {
    for (name, text) in &receipt.outputs {
        let path = output(receipt, name);
        if files::read(&path)?.as_ref() == Some(text) {
            files::writable(&path)?;
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

pub(super) fn set(
    env: &Environment,
    config: &crate::config::Config,
    enabled: bool,
    palette: &Value,
    dark: bool,
) -> Result<()> {
    let saved = load(env)?;
    let status = inspect(env, config);
    if !enabled {
        if let Some(mut receipt) = saved.filter(|r| r.enabled || r.pending) {
            ensure!(status.can_disable, "The Plasma theme changed outside Skwd; review its setup");
            receipt.pending = true;
            save(env, &receipt)?;
            apply_scheme(env, &receipt.previous)?;
            remove_outputs(&receipt)?;
            receipt.enabled = false;
            receipt.pending = false;
            save(env, &receipt)?;
        }
        return Ok(());
    }
    ensure!(
        status.can_enable || status.enabled && status.can_disable && status.state == "applied",
        "Plasma theme setup is unavailable: {} {}",
        status.state,
        status.detail
    );
    let mut receipt = match saved.filter(|r| r.enabled) {
        Some(receipt) => receipt,
        None => Receipt {
            version: 1,
            config: env.config.join("kdeglobals"),
            directory: env.data.join("color-schemes"),
            previous: selected(env)?,
            active: String::new(),
            outputs: BTreeMap::new(),
            enabled: false,
            pending: false,
        },
    };
    let name = if selected(env)? == NAMES[0] { NAMES[1] } else { NAMES[0] };
    let rendered = if receipt.enabled && !super::super::profiles::valid_palette(palette) {
        receipt
            .outputs
            .get(&receipt.active)
            .context("The saved Plasma colours are missing")?
            .replace(&receipt.active, name)
    } else {
        manager::rendered(&KDE_RECIPE, palette, dark)?.replace("SkwdMatugen", name)
    };
    let path = output(&receipt, name);
    ensure!(
        files::read(&path)?.as_ref() == receipt.outputs.get(name),
        "The generated Plasma theme was changed outside Skwd"
    );
    receipt.active = name.into();
    receipt.outputs.insert(name.into(), rendered.clone());
    receipt.pending = true;
    save(env, &receipt)?;
    files::write(&path, &rendered)?;
    apply_scheme(env, name)?;
    receipt.enabled = true;
    receipt.pending = false;
    save(env, &receipt)
}

pub(super) fn update(
    env: &Environment,
    config: &crate::config::Config,
    palette: &Value,
    dark: bool,
) -> Result<()> {
    if load(env)?.is_some_and(|receipt| receipt.enabled && !receipt.pending) {
        set(env, config, true, palette, dark)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "plasma_tests.rs"]
mod tests;
