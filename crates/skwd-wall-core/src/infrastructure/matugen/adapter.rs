use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::config::Config;

pub fn notify_change(config: &Config) {
    if config.notify_on_change() {
        run_sh_detached(
            "command -v notify-send >/dev/null && notify-send 'Wallpaper Changed' || true",
        );
    }
}

fn matugen_available() -> bool {
    crate::theme::cli_available("matugen")
}

pub(crate) fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

pub(crate) fn run_sh(cmd: &str) {
    let _ = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

pub(crate) fn run_sh_detached(cmd: &str) {
    let detached = Command::new("setsid")
        .arg("-f")
        .arg("sh")
        .arg("-c")
        .arg(cmd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if detached.is_ok() {
        return;
    }
    let mut fallback = Command::new("sh");
    fallback.arg("-c").arg(cmd).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    crate::proc::spawn_reaped(&mut fallback, "detached hook");
}

pub fn generate_config(config: &Config) -> PathBuf {
    let cache = PathBuf::from(config.cache_dir());
    let config_path = cache.join("matugen").join("config.toml");
    let template_dir = config.theme().templates_dir();

    let mut lines = vec!["[config]".to_string(), "reload_apps = false".to_string(), String::new()];
    let mut emitted = 0usize;
    for (idx, integ) in config.theme().integrations().iter().enumerate() {
        if integ.template.is_empty() || integ.output.is_empty() {
            continue;
        }
        let input_path = if integ.template.contains('/') {
            PathBuf::from(config.resolve(&integ.template))
        } else {
            template_dir.join(&integ.template)
        };
        if !input_path.exists() {
            log::warn!("matugen template not found: {}", input_path.display());
            continue;
        }
        let output_path = if integ.output.contains('/') {
            PathBuf::from(config.resolve(&integ.output))
        } else {
            cache.join(&integ.output)
        };
        lines.push(format!("[templates.integration_{idx}]"));
        lines.push(format!("input_path = \"{}\"", input_path.display()));
        lines.push(format!("output_path = \"{}\"", output_path.display()));
        lines.push(String::new());
        emitted += 1;
    }
    if emitted == 0 {
        lines.push("[templates]".to_string());
    }

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&config_path, lines.join("\n")).ok();
    config_path
}

pub fn run(config: &Config, image_path: &str) -> bool {
    run_with(config, image_path, None, None, None)
}

fn invoke(
    config: &Config,
    image_path: &str,
    config_path: &Path,
    scheme: &str,
    mode: &str,
    index: u32,
) -> bool {
    let mut cmd = Command::new("matugen");
    cmd.arg("-c")
        .arg(config_path)
        .arg("image")
        .arg("-t")
        .arg(scheme)
        .arg("-m")
        .arg(mode)
        .arg("--source-color-index")
        .arg(index.to_string());
    if let Some(contrast) = config.theme().matugen_contrast() {
        cmd.arg("--contrast").arg(contrast.to_string());
    }
    let output = cmd
        .arg("-j")
        .arg("hex")
        .arg(image_path)
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output();
    match output {
        Ok(out) if out.status.success() => {
            log::info!("matugen ok for {image_path} (--source-color-index {index})");
            match serde_json::from_slice::<serde_json::Value>(&out.stdout) {
                Ok(val) => {
                    let published = crate::theme::matugen_source(&val).is_some_and(|seed| {
                        crate::theme::publish_scheme(config, &seed, mode != "light")
                    });
                    if !published {
                        crate::theme::write_picker_palette_cols(
                            config,
                            &crate::theme::matugen_pick(&val),
                        );
                    }
                }
                Err(err) => {
                    log::warn!("matugen -j hex unparseable, picker colours not updated: {err}");
                }
            }
            true
        }
        Ok(out) => {
            log::warn!(
                "matugen exited {} for {image_path} (--source-color-index {index})",
                out.status
            );
            false
        }
        Err(err) => {
            log::warn!("matugen failed: {err}");
            false
        }
    }
}

pub(crate) fn resolve_cli_mode(config: &Config, image: &str, mode: &str) -> String {
    if mode != "auto" {
        return mode.to_string();
    }
    if crate::theme::resolve_dark(config, image) { "dark".to_string() } else { "light".to_string() }
}

pub fn run_with(
    config: &Config,
    image_path: &str,
    scheme: Option<&str>,
    mode: Option<&str>,
    index: Option<u32>,
) -> bool {
    if !config.theme().matugen_enabled() {
        log::warn!(
            "theme backend is matugen but features.matugen is off; theming did nothing for {image_path}"
        );
        return false;
    }
    if !matugen_available() {
        log::warn!("matugen not found in PATH, skipping theming");
        return false;
    }

    let config_path = generate_config(config);
    let scheme = scheme.map_or_else(|| config.theme().matugen_scheme(), str::to_string);
    let mode = mode.map_or_else(|| config.theme().matugen_mode(), str::to_string);
    let mode = resolve_cli_mode(config, image_path, &mode);
    let want = index.unwrap_or_else(|| config.theme().matugen_color_index());

    let mut used = want;
    let mut ok = invoke(config, image_path, &config_path, &scheme, &mode, want);
    if !ok && want != 0 {
        log::warn!(
            "matugen failed at --source-color-index {want} (the image may expose fewer source colours); retrying with 0"
        );
        used = 0;
        ok = invoke(config, image_path, &config_path, &scheme, &mode, 0);
    }
    if !ok {
        log::warn!(
            "matugen produced no colours for {image_path}; integration reloads skipped so apps keep their current theme"
        );
        return false;
    }

    if let Some(default_cfg) = config.theme().default_matugen_config()
        && Path::new(&default_cfg).exists()
    {
        let default_cmd =
            "matugen -c %config% image %path% -t %scheme% -m %mode% --source-color-index %index%";
        let template =
            config.theme().external_matugen_command().unwrap_or_else(|| default_cmd.to_string());
        let cmd_str = template
            .replace("%config%", &shell_quote(&default_cfg))
            .replace("%path%", &shell_quote(image_path))
            .replace("%scheme%", &shell_quote(&scheme))
            .replace("%mode%", &shell_quote(&mode))
            .replace("%index%", &used.to_string());
        run_sh(&cmd_str);
    }

    run_reloads(config);
    true
}

pub(crate) fn run_reloads(config: &Config) {
    run_reloads_where(config, |_| true);
}

pub(crate) fn run_reloads_where(
    config: &Config,
    keep: impl Fn(&crate::config::Integration) -> bool,
) {
    for integ in config.theme().integrations() {
        if integ.reload.is_empty() || !keep(&integ) {
            continue;
        }
        let resolved = config.resolve(&integ.reload);
        let cmd = if resolved.contains('/') && !resolved.contains(' ') {
            format!("sh {}", shell_quote(&resolved))
        } else {
            integ.reload.clone()
        };
        run_sh_detached(&cmd);
    }
}

#[path = "tests.rs"]
mod tests;
