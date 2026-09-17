use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use anyhow::{Context, Result, ensure};
use serde_json::Value;
use wall_proto::{AppThemeStatus, AppThemesResult};

use super::catalogue::{KDE_RECIPE, RECIPES, Recipe};
use super::files::{self, Receipt};
use super::reload::reload;

static SERIAL: Mutex<()> = Mutex::new(());

pub(super) struct Environment {
    pub home: PathBuf,
    pub config: PathBuf,
    pub data: PathBuf,
    pub data_dirs: Vec<PathBuf>,
    pub receipts: PathBuf,
    pub search: Vec<PathBuf>,
    pub reload: bool,
}

impl Environment {
    pub(super) fn current() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join(".config"));
        let state = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join(".local/state"));
        Self {
            home: home.clone(),
            config,
            data: std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| home.join(".local/share")),
            data_dirs: std::env::split_paths(
                &std::env::var_os("XDG_DATA_DIRS")
                    .unwrap_or_else(|| "/usr/local/share:/usr/share".into()),
            )
            .collect(),
            receipts: state.join("skwd-wall-v2/app-themes"),
            search: std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect(),
            reload: true,
        }
    }

    pub(super) fn executable(&self, id: &str) -> Option<PathBuf> {
        self.search.iter().map(|dir| dir.join(id)).find(|path| {
            std::fs::metadata(path)
                .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        })
    }

    fn paths(&self, recipe: &Recipe) -> (PathBuf, PathBuf, PathBuf) {
        let mut config = self.config.join(recipe.config);
        if recipe.id == "ghostty" && self.config.join("ghostty/config.ghostty").exists() {
            config = self.config.join("ghostty/config.ghostty");
        }
        let output = config.parent().unwrap_or(&self.config).join(recipe.output);
        (config, output, self.receipts.join(format!("{}.json", recipe.id)))
    }

    fn check_receipt(&self, recipe: &Recipe, receipt: &Receipt) -> Result<()> {
        let (config, output, _) = self.paths(recipe);
        ensure!(
            receipt.config == config && receipt.output == output,
            "The app config location changed; review its saved setup"
        );
        Ok(())
    }
}

pub(super) fn conflict(
    config: &crate::config::Config,
    recipe: &Recipe,
    text: &str,
) -> Option<String> {
    if config.theme().integrations().iter().any(|entry| entry.name.eq_ignore_ascii_case(recipe.id))
    {
        return Some("custom-output".into());
    }
    let migrated = config.theme().disabled_integration(recipe.id);
    let lower = text.to_ascii_lowercase();
    ["noctalia", "dank", "matugen"]
        .into_iter()
        .filter(|name| *name != "matugen" || !migrated)
        .find(|name| {
            lower
                .lines()
                .filter(|line| !line.trim_start().starts_with(['#', '/']))
                .any(|line| line.contains(name))
        })
        .map(str::to_string)
}

fn inspect(env: &Environment, config: &crate::config::Config, recipe: &Recipe) -> AppThemeStatus {
    let (path, output, receipt_path) = env.paths(recipe);
    let installed = env.executable(recipe.id).is_some();
    let mut status = AppThemeStatus {
        id: recipe.id.into(),
        name: recipe.name.into(),
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
    let check = || -> Result<(String, String, bool, bool)> {
        files::writable(&path)?;
        files::writable(&output)?;
        let text = files::read(&path)?;
        ensure!(
            recipe.id != "waybar" || text.is_some(),
            "Waybar has no style.css yet; copy the default stylesheet first"
        );
        let text = text.unwrap_or_default();
        if let Some(receipt) = files::load(&receipt_path)? {
            env.check_receipt(recipe, &receipt)?;
            if receipt.enabled || receipt.pending {
                let valid = files::restore(&receipt, recipe, &text).is_ok()
                    || receipt.pending
                        && (receipt.pending_config.as_deref() == Some(&text)
                            || !receipt.enabled
                                && receipt.original.as_deref().unwrap_or_default() == text);
                let output_valid =
                    files::read(&output)?.as_deref() == Some(receipt.rendered.as_str());
                return Ok((
                    if receipt.pending {
                        "interrupted"
                    } else if !valid || !output_valid {
                        "changed"
                    } else {
                        &receipt.result
                    }
                    .into(),
                    String::new(),
                    receipt.enabled,
                    valid,
                ));
            }
        }
        if let Some(owner) = conflict(config, recipe, &text) {
            return Ok(("conflict".into(), owner, false, false));
        }
        ensure!(!output.exists(), "A theme file already exists at {}", output.display());
        Ok((status.state.clone(), String::new(), false, false))
    };
    match check() {
        Ok((state, detail, enabled, can_disable)) => {
            status.can_enable &= state == "ready";
            status.state = state;
            status.detail = detail;
            status.enabled = enabled;
            status.can_disable = can_disable;
        }
        Err(error) => {
            status.state = "needs-review".into();
            status.detail = error.to_string();
            status.can_enable = false;
        }
    }
    status.can_adopt = status.state == "conflict"
        && status.detail == "custom-output"
        && migratable(config, recipe);
    status
}

pub fn list(config: &crate::config::Config) -> AppThemesResult {
    let _serial = crate::lock(&SERIAL);
    list_with(&Environment::current(), config)
}

pub(super) fn list_with(env: &Environment, config: &crate::config::Config) -> AppThemesResult {
    let mut apps: Vec<_> = RECIPES.iter().map(|recipe| inspect(env, config, recipe)).collect();
    apps.push(super::plasma::inspect(env, config));
    apps.extend(super::structured::APPS.iter().map(|app| app.inspect(env, config)));
    apps.sort_by(|a, b| b.installed.cmp(&a.installed).then_with(|| a.name.cmp(&b.name)));
    AppThemesResult { apps }
}

pub(super) fn rendered(recipe: &Recipe, palette: &Value, dark: bool) -> Result<String> {
    ensure!(
        super::super::profiles::valid_palette(palette),
        "Apply a wallpaper or choose a colour theme first"
    );
    let text = crate::static_templates::render_palette(recipe.template, palette, dark);
    ensure!(!text.contains("{{"), "The app template contains unsupported colour tokens");
    Ok(text)
}

fn validate(env: &Environment, recipe: &Recipe, path: &Path, text: &str) -> Result<()> {
    if recipe.id != "niri" || !env.reload {
        return Ok(());
    }
    let parent = path.parent().context("Missing Niri config directory")?;
    let mut staged =
        tempfile::Builder::new().prefix(".skwd-validate-").suffix(".kdl").tempfile_in(parent)?;
    staged.write_all(text.as_bytes())?;
    let result = Command::new(env.executable("niri").context("Niri is not installed")?)
        .arg("validate")
        .arg("--config")
        .arg(staged.path())
        .stdin(Stdio::null())
        .output()?;
    ensure!(
        result.status.success(),
        "Niri rejected the theme setup: {}",
        String::from_utf8_lossy(&result.stderr).trim()
    );
    Ok(())
}

pub fn set_enabled(
    state: &crate::WallState,
    id: &str,
    enabled: bool,
    adopt: bool,
) -> Result<AppThemesResult> {
    let _theme = state.theme().lock_shell_preview();
    let _serial = crate::lock(&SERIAL);
    let env = Environment::current();
    let config = state.config().clone();
    let snapshot = state.theme().applied_theme();
    let mut palette =
        snapshot.as_ref().and_then(|value| value.get("palette")).cloned().unwrap_or(Value::Null);
    if let Some(scheme) = snapshot.as_ref().and_then(|value| value.get("scheme")) {
        palette["_scheme"] = scheme.clone();
    }
    let dark = snapshot.as_ref().and_then(|value| value["dark"].as_bool()).unwrap_or(true);
    if adopt {
        ensure!(enabled, "Migration requires enabling the app theme");
        let structured = super::structured::APPS.iter().find(|app| app.id == id);
        let can_migrate = structured.map_or_else(
            || {
                RECIPES
                    .iter()
                    .chain(std::iter::once(&KDE_RECIPE))
                    .find(|recipe| recipe.id == id)
                    .is_some_and(|recipe| migratable(&config, recipe))
            },
            |app| app.migratable(&config),
        );
        ensure!(can_migrate, "The custom output needs manual review");
        let path = crate::config::config_path();
        let original = files::read(&path)?.context("Wall config is missing")?;
        let mut document: Value = serde_json::from_str(&original)?;
        let entries = document
            .get_mut("integrations")
            .and_then(Value::as_array_mut)
            .context("No custom outputs found")?;
        for entry in entries {
            if entry["name"].as_str().is_some_and(|name| {
                name.eq_ignore_ascii_case(id) || structured.is_some_and(|app| app.matches(name))
            }) {
                entry["enabled"] = Value::Bool(false);
            }
        }
        let next = serde_json::to_string_pretty(&document)?;
        let migration = env.receipts.join(format!("{id}-migration.json"));
        files::write(
            &migration,
            &serde_json::to_string_pretty(
                &serde_json::json!({"config_before": original, "config_after": next}),
            )?,
        )?;
        ensure!(
            files::read(&path)?.as_deref() == Some(&original),
            "Wall config changed during migration; try again"
        );
        files::write(&path, &next)?;
        let migrated = crate::config::Config::from_root(document);
        if let Err(error) = set_with(&env, &migrated, id, true, &palette, dark) {
            if files::read(&path)?.as_deref() == Some(&next) {
                files::write(&path, &original)?;
            }
            return Err(error);
        }
        state.reload_config();
    } else {
        set_with(&env, &config, id, enabled, &palette, dark)?;
    }
    Ok(list_with(&env, &state.config()))
}

pub(super) fn migratable(config: &crate::config::Config, recipe: &Recipe) -> bool {
    let entries = config.theme().integrations();
    let matching: Vec<_> =
        entries.iter().filter(|entry| entry.name.eq_ignore_ascii_case(recipe.id)).collect();
    !matching.is_empty()
        && matching.iter().all(|entry| {
            let expected = match recipe.id {
                "btop" => "btop.theme",
                "niri" => "niri-colors.kdl",
                "ghostty" => "ghostty.conf",
                "kde" => "kde-colors.colors",
                "rofi" => "rofi.rasi",
                "waybar" => "waybar.css",
                _ => "kitty.conf",
            };
            Path::new(&entry.template).file_name().is_some_and(|name| name == expected)
        })
}

pub(super) fn set_with(
    env: &Environment,
    config: &crate::config::Config,
    id: &str,
    enabled: bool,
    palette: &Value,
    dark: bool,
) -> Result<()> {
    if let Some(app) = super::structured::APPS.iter().find(|app| app.id == id) {
        return app.set(env, config, enabled, palette, dark);
    }
    if id == "kde" {
        return super::plasma::set(env, config, enabled, palette, dark);
    }
    let recipe = RECIPES
        .iter()
        .chain(std::iter::once(&KDE_RECIPE))
        .find(|recipe| recipe.id == id)
        .context("Unknown app theme")?;
    let (path, output, receipt_path) = env.paths(recipe);
    let saved = files::load(&receipt_path)?;
    if let Some(receipt) = &saved {
        env.check_receipt(recipe, receipt)?;
    }
    if enabled {
        if let Some(mut receipt) =
            saved.clone().filter(|receipt| receipt.enabled && !receipt.pending)
        {
            let status = inspect(env, config, recipe);
            ensure!(
                status.can_disable && status.state != "changed",
                "The app theme changed outside Skwd; review its setup"
            );
            receipt.result = reload(env, recipe);
            return files::save(&receipt_path, &receipt);
        }
        let status = inspect(env, config, recipe);
        ensure!(
            status.can_enable,
            "Theme setup is unavailable: {} {}",
            status.state,
            status.detail
        );
        let original = files::read(&path)?;
        let text = original.as_deref().unwrap_or_default();
        let (before, after) = files::patch(recipe, text)?;
        let next = files::changed(text, &before, &after)?;
        let mut receipt = Receipt {
            version: 1,
            enabled: false,
            pending: true,
            pending_config: Some(next.clone()),
            config: path.clone(),
            output: output.clone(),
            original,
            before,
            after,
            rendered: rendered(recipe, palette, dark)?,
            result: "configured".into(),
        };
        files::save(&receipt_path, &receipt)?;
        let install = || -> Result<()> {
            files::write(&output, &receipt.rendered)?;
            validate(env, recipe, &path, &next)?;
            ensure!(
                files::read(&path)? == receipt.original,
                "The app config changed during setup; try again"
            );
            files::write(&path, &next)
        };
        if let Err(error) = install() {
            if files::read(&output)?.as_deref() == Some(&receipt.rendered) {
                std::fs::remove_file(&output)?;
            }
            receipt.pending = false;
            files::save(&receipt_path, &receipt)?;
            return Err(error);
        }
        receipt.enabled = true;
        receipt.pending = false;
        files::save(&receipt_path, &receipt)?;
        receipt.result = reload(env, recipe);
        files::save(&receipt_path, &receipt)?;
    } else if let Some(mut receipt) = saved.filter(|receipt| receipt.enabled || receipt.pending) {
        let current = files::read(&path)?.unwrap_or_default();
        let restored = if receipt.pending
            && (receipt.pending_config.as_deref() == Some(&current) && receipt.enabled
                || !receipt.enabled && receipt.original.as_deref().unwrap_or_default() == current)
        {
            current.clone()
        } else {
            files::restore(&receipt, recipe, &current)?
        };
        validate(env, recipe, &path, &restored)?;
        receipt.pending = true;
        receipt.pending_config = Some(restored.clone());
        files::save(&receipt_path, &receipt)?;
        if restored.is_empty() && receipt.original.is_none() {
            files::writable(&path)?;
            if path.exists() {
                std::fs::remove_file(&path)?;
            }
        } else {
            files::write(&path, &restored)?;
        }
        if files::read(&output)?.as_deref() == Some(&receipt.rendered) {
            files::writable(&output)?;
            std::fs::remove_file(&output)?;
        }
        receipt.enabled = false;
        receipt.pending = false;
        receipt.result = reload(env, recipe);
        files::save(&receipt_path, &receipt)?;
    }
    Ok(())
}

pub fn apply(config: &crate::config::Config, palette: &Value, dark: bool) {
    let _serial = crate::lock(&SERIAL);
    apply_with(&Environment::current(), config, palette, dark);
}

pub(super) fn apply_with(
    env: &Environment,
    config: &crate::config::Config,
    palette: &Value,
    dark: bool,
) {
    if !env.receipts.exists() {
        return;
    }
    if let Err(error) = super::plasma::update(env, config, palette, dark) {
        log::warn!("app theme kde: {error:#}");
    }
    for app in &super::structured::APPS {
        if app.inspect(env, config).enabled
            && let Err(error) = app.set(env, config, true, palette, dark)
        {
            log::warn!("app theme {}: {error:#}", app.id);
        }
    }
    for recipe in &RECIPES {
        let (_, _, path) = env.paths(recipe);
        let update = || -> Result<()> {
            let Some(mut receipt) =
                files::load(&path)?.filter(|receipt| receipt.enabled && !receipt.pending)
            else {
                return Ok(());
            };
            env.check_receipt(recipe, &receipt)?;
            let current = files::read(&receipt.config)?.context("The app config was removed")?;
            files::restore(&receipt, recipe, &current)?;
            ensure!(
                files::read(&receipt.output)?.as_deref() == Some(&receipt.rendered),
                "The generated theme was edited outside Skwd"
            );
            let text = rendered(recipe, palette, dark)?;
            if text == receipt.rendered && receipt.result != "reload-needed" {
                return Ok(());
            }
            files::write(&receipt.output, &text)?;
            receipt.rendered = text;
            receipt.result = reload(env, recipe);
            files::save(&path, &receipt)
        };
        if let Err(error) = update() {
            log::warn!("app theme {}: {error:#}", recipe.id);
        }
    }
}
