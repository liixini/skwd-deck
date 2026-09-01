use std::fmt::Write;

use crate::config::Config;
use crate::paths;

pub fn rss_mb() -> u64 {
    pid_rss_kb(std::process::id()) / 1024
}

pub(crate) use skwd_log::proc::pid_rss_kb;
pub use skwd_log::proc::{MemBreakdown, mem_breakdown};

pub fn vram_mb_for(pids: &[u32]) -> u64 {
    if pids.is_empty() {
        return 0;
    }
    let out = match std::process::Command::new("nvidia-smi").output() {
        Ok(out) if out.status.success() => out.stdout,
        _ => return 0,
    };
    parse_vram(&String::from_utf8_lossy(&out), pids)
}

fn parse_vram(text: &str, pids: &[u32]) -> u64 {
    let mut total = 0u64;
    for line in text.lines() {
        if !line.contains("MiB") {
            continue;
        }
        let toks: Vec<&str> = line.split_whitespace().filter(|tok| *tok != "|").collect();
        let Some(tidx) = toks.iter().position(|tok| matches!(*tok, "G" | "C" | "C+G" | "G+C"))
        else {
            continue;
        };
        if tidx == 0 {
            continue;
        }
        let Some(pid) = toks[tidx - 1].parse::<u32>().ok() else {
            continue;
        };
        if !pids.contains(&pid) {
            continue;
        }
        if let Some(mem) = toks
            .last()
            .and_then(|tok| tok.strip_suffix("MiB"))
            .and_then(|num| num.parse::<u64>().ok())
        {
            total += mem;
        }
    }
    total
}

pub fn config_report(config: &Config) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "config + paths:");
    let _ = writeln!(out, "  wallpaper_dir = {}", config.wallpaper_dir());
    let _ = writeln!(out, "  video_dir     = {}", config.video_dir());
    let _ = writeln!(out, "  we_dir        = {}", config.we_dir().display());
    let _ = writeln!(out, "  cache_dir     = {}", config.cache_dir());
    let _ = writeln!(out, "  thumbs_dir    = {}", paths::thumbs_dir().display());
    let _ = writeln!(out, "  db_path       = {}", paths::db_path().display());
    let _ = writeln!(out, "  still_bin     = {}", config.renderer().still_bin());
    let _ = writeln!(out, "  paper_vk_bin  = {}", config.renderer().vk_bin());
    let _ = writeln!(out, "  fill_mode     = {}", config.display().fill_mode());
    let _ = writeln!(
        out,
        "  audio         = mute={} volume={}",
        config.renderer().mute(),
        config.renderer().volume()
    );
    let _ = writeln!(
        out,
        "  matugen       = enabled={} scheme={} mode={} index={} contrast={:?}",
        config.theme().matugen_enabled(),
        config.theme().matugen_scheme(),
        config.theme().matugen_mode(),
        config.theme().matugen_color_index(),
        config.theme().matugen_contrast()
    );
    let _ = writeln!(
        out,
        "  steam/WE      = enabled={} scaling={} fps={}",
        config.steam_enabled(),
        config.renderer().we_scaling(),
        config.renderer().we_fps()
    );
    out
}

pub fn env_report() -> String {
    let mut out = String::from("env:\n");
    let keys = [
        "WAYLAND_DISPLAY",
        "XDG_RUNTIME_DIR",
        "XDG_CONFIG_HOME",
        "XDG_CACHE_HOME",
        "XDG_DATA_HOME",
        "SKWD_WALL_V2_CONFIG",
        "SKWD_WALL_V2_CACHE",
        "SKWD_WALL_V2_SOCK",
        "SKWD_WALL_PAPER_STILL",
        "SKWD_WALL_PAPER_VK",
        "SKWD_WALL_DEBUG",
        "SKWD_WALL_LOG",
        "SKWD_WALL_MODE",
        "SKWD_WALL_HUD",
        "SKWD_WALL_METRICS",
        "SKWD_WALL_OUTPUT",
        "SKWD_WALL_AUTO_EXIT_MS",
    ];
    for key in keys {
        let val = std::env::var(key).unwrap_or_else(|_| "(unset)".into());
        let _ = writeln!(out, "  {key} = {val}");
    }
    out
}

#[path = "tests.rs"]
mod tests;
