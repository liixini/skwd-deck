use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, bail};
use serde_json::Value;

use crate::config::Config;

fn shell_dir(config_dir: &Path) -> anyhow::Result<PathBuf> {
    let valid = |path: &Path| path.join("matugen/configs/base.toml").is_file();
    if let Some(path) = skwd_config::env("DMS_SHELL_DIR").filter(|path| !path.is_empty()) {
        let path = PathBuf::from(path);
        anyhow::ensure!(valid(&path), "DMS_SHELL_DIR has no Matugen templates");
        return Ok(path);
    }
    if let Some(runtime) = skwd_config::env("XDG_RUNTIME_DIR") {
        for name in ["danklinux.path", "dms.path"] {
            if let Ok(path) = std::fs::read_to_string(Path::new(&runtime).join(name)) {
                let path = PathBuf::from(path.trim());
                if valid(&path) {
                    return Ok(path);
                }
            }
        }
    }
    for path in [
        config_dir.join("quickshell/dms"),
        PathBuf::from("/etc/xdg/quickshell/dms"),
        PathBuf::from("/usr/share/quickshell/dms"),
        PathBuf::from("/usr/local/share/quickshell/dms"),
    ] {
        if valid(&path) {
            return Ok(path);
        }
    }
    bail!("DMS shell templates were not found; start DMS or set DMS_SHELL_DIR")
}

fn skip_templates(settings: &Value, checks: &Value) -> anyhow::Result<String> {
    let checks = checks.as_array().context("DMS returned an invalid app template list")?;
    anyhow::ensure!(!checks.is_empty(), "DMS returned an empty app template list");
    let run = settings.get("runDmsMatugenTemplates").and_then(Value::as_bool).unwrap_or(true);
    let mut skipped = Vec::new();
    for check in checks {
        let id = check.get("id").and_then(Value::as_str).context("DMS template has no id")?;
        anyhow::ensure!(
            !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'),
            "invalid DMS template id"
        );
        let suffix = if id == "nvim" { "neovim" } else { id };
        let enabled = settings.as_object().and_then(|settings| {
            settings.iter().find_map(|(key, value)| {
                key.strip_prefix("matugenTemplate")
                    .filter(|key| key.eq_ignore_ascii_case(suffix))
                    .and_then(|_| value.as_bool())
            })
        });
        if !run || !enabled.unwrap_or(id != "nvim") {
            skipped.push(id);
        }
    }
    Ok(skipped.join(","))
}

fn generation_args(
    settings: &Value,
    checks: &Value,
    image: &str,
    dark: bool,
) -> anyhow::Result<Vec<String>> {
    let mut args = vec![
        "matugen".into(),
        "queue".into(),
        "--kind".into(),
        "image".into(),
        "--value".into(),
        image.into(),
        "--mode".into(),
        if settings.get("matugenSmartMode").and_then(Value::as_bool).unwrap_or(false) {
            "smart"
        } else if dark {
            "dark"
        } else {
            "light"
        }
        .into(),
        "--matugen-type".into(),
        super::scheme_from_settings(Some(settings)),
        "--icon-theme".into(),
        settings.get("iconTheme").and_then(Value::as_str).unwrap_or("System Default").into(),
    ];
    if settings.get("runUserMatugenTemplates").and_then(Value::as_bool) == Some(false) {
        args.push("--run-user-templates=false".into());
    }
    for (key, flag) in [
        ("syncModeWithPortal", "--sync-mode-with-portal"),
        ("terminalsAlwaysDark", "--terminals-always-dark"),
    ] {
        if settings.get(key).and_then(Value::as_bool) == Some(true) {
            args.push(flag.into());
        }
    }
    if let Some(contrast) =
        settings.get("matugenContrast").and_then(Value::as_f64).filter(|v| *v != 0.0)
    {
        args.extend(["--contrast".into(), contrast.to_string()]);
    }
    if let Some(mode) = settings
        .get("matugenSourceMode")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty() && *v != "dominant")
    {
        args.extend(["--source-mode".into(), mode.into()]);
    }
    let skipped = skip_templates(settings, checks)?;
    if !skipped.is_empty() {
        args.extend(["--skip-templates".into(), skipped]);
    }
    Ok(args)
}

pub(super) fn apply(config: &Config, image: &str, dark: bool) -> anyhow::Result<()> {
    anyhow::ensure!(
        !skwd_config::env("DMS_DISABLE_MATUGEN").is_some_and(|v| v == "1" || v == "true"),
        "DMS theme generation is disabled by DMS_DISABLE_MATUGEN"
    );
    let settings_path = super::settings_path_from(
        skwd_config::env("XDG_CONFIG_HOME").as_deref(),
        &skwd_config::home(),
    );
    let config_dir =
        settings_path.parent().and_then(Path::parent).context("DMS config directory missing")?;
    let mut settings: Value = match std::fs::read(&settings_path) {
        Ok(bytes) => serde_json::from_slice(&bytes).context("invalid DMS settings")?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
        Err(error) => return Err(error.into()),
    };
    anyhow::ensure!(settings.is_object(), "invalid DMS settings object");
    if let Some(scheme) = config.theme().matugen_scheme_override() {
        settings["matugenScheme"] = Value::String(scheme);
    }
    let shell = shell_dir(config_dir)?;
    let checks = Command::new("dms")
        .args(["matugen", "check"])
        .stdin(Stdio::null())
        .output()
        .context("DMS template discovery failed")?;
    anyhow::ensure!(
        checks.status.success(),
        "DMS template discovery: {}",
        String::from_utf8_lossy(&checks.stderr)
    );
    let checks: Value = serde_json::from_slice(&checks.stdout)
        .context("invalid DMS template discovery response")?;
    let args = generation_args(&settings, &checks, image, dark)?;
    let colors = super::colors_path_from(
        skwd_config::env("XDG_CACHE_HOME").as_deref(),
        &skwd_config::home(),
    );
    let result = Command::new("dms")
        .args(args)
        .arg("--state-dir")
        .arg(colors.parent().context("DMS cache directory missing")?)
        .arg("--shell-dir")
        .arg(shell)
        .arg("--config-dir")
        .arg(config_dir)
        .stdin(Stdio::null())
        .output()
        .context("DMS theme generation failed")?;
    anyhow::ensure!(
        result.status.success() || result.status.code() == Some(2),
        "DMS theme generation: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(())
}

#[cfg(test)]
#[path = "generation_tests.rs"]
mod tests;
