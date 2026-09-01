use std::path::Path;
use std::process::{Command, Stdio};

use crate::config::Config;

pub fn substitute(template: &str, wp_type: &str, name: &str, path: &str, thumb: &str) -> String {
    template
        .replace("%type%", &shell_quote(wp_type))
        .replace("%name%", &shell_quote(name))
        .replace("%path%", &shell_quote(path))
        .replace("%thumb%", &shell_quote(thumb))
}

pub fn basename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map_or_else(|| path.to_string(), |name| name.to_string_lossy().into_owned())
}

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

fn run_detached(cmd: &str) {
    let wrapped = format!("nohup setsid sh -c {} </dev/null >/dev/null 2>&1 &", shell_quote(cmd));
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(&wrapped).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    crate::proc::spawn_reaped(&mut cmd, "post-processing");
}

pub fn run(config: &Config, wp_type: &str, path: &str, thumb: &str, restoring: bool) {
    if restoring && !config.post_process_on_restore() {
        return;
    }
    let name = basename(path);
    for (cmd, ty) in config.post_processing() {
        if ty != "all" && ty != wp_type {
            continue;
        }
        let resolved = substitute(&cmd, wp_type, &name, path, thumb);
        log::info!("post-processing ({wp_type}): {resolved}");
        run_detached(&resolved);
    }
}

#[path = "tests.rs"]
mod tests;
