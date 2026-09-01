use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use skwd_wall_core::config::{Config, config_path};
use wall_proto::socket_path;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Pass,
    Warn,
    Fail,
}

impl Status {
    fn marker(self) -> &'static str {
        match self {
            Status::Pass => "\x1b[32m✓\x1b[0m",
            Status::Warn => "\x1b[33m!\x1b[0m",
            Status::Fail => "\x1b[31m✗\x1b[0m",
        }
    }
    fn word(self) -> &'static str {
        match self {
            Status::Pass => "pass",
            Status::Warn => "warn",
            Status::Fail => "fail",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LayerShell {
    Yes,
    No,
    Unknown,
}

pub fn layer_shell_support(desktop: &str) -> LayerShell {
    const WLR: &[&str] = &[
        "niri", "sway", "hyprland", "hypr", "river", "wayfire", "labwc", "cosmic", "miracle", "dwl",
    ];
    let desk = desktop.to_lowercase();
    if WLR.iter().any(|comp| desk.contains(comp)) || desk.contains("kde") || desk.contains("plasma")
    {
        return LayerShell::Yes;
    }
    if desk.contains("gnome") || desk.contains("unity") {
        return LayerShell::No;
    }
    LayerShell::Unknown
}

pub fn pickonly_trap(pick_only: bool, postproc_count: usize) -> bool {
    pick_only && postproc_count == 0
}

pub fn solar_coords_unset(schedule_enabled: bool, solar: bool, lat: f64, lon: f64) -> bool {
    schedule_enabled && solar && lat.abs() < 1e-9 && lon.abs() < 1e-9
}

pub fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|dir| dir.join(name)).find(|cand| cand.is_file())
}

fn find_bin(name: &str) -> Option<PathBuf> {
    which(name).or_else(|| {
        let cand = std::env::current_exe().ok()?.parent()?.join(name);
        cand.is_file().then_some(cand)
    })
}

struct Report {
    lines: Vec<(Status, String, String)>,
}

impl Report {
    fn new() -> Self {
        Self { lines: Vec::new() }
    }
    fn push(&mut self, status: Status, label: &str, detail: impl Into<String>) {
        self.lines.push((status, label.to_string(), detail.into()));
    }
    fn exit_code(&self) -> i32 {
        i32::from(self.lines.iter().any(|(status, ..)| *status == Status::Fail))
    }
    fn to_json(&self) -> serde_json::Value {
        let arr: Vec<serde_json::Value> = self
            .lines
            .iter()
            .map(|(status, label, detail)| {
                serde_json::json!({ "status": status.word(), "check": label, "detail": detail })
            })
            .collect();
        serde_json::json!({ "checks": arr })
    }
}

fn build_report() -> Report {
    let cfg = Config::load();
    let mut report = Report::new();

    check_session(&mut report);
    check_renderers(&mut report, &cfg);
    check_socket(&mut report);
    check_config_file(&mut report);
    check_traps(&mut report, &cfg);
    check_wallpaper_dir(&mut report, &cfg);
    check_cache(&mut report, &cfg);
    check_theme_backend(&mut report, &cfg);
    check_optional_bins(&mut report, &cfg);
    check_sibling_bins(&mut report);
    check_templates(&cfg, &mut report);

    report
}

fn check_session(report: &mut Report) {
    let env = |key: &str| std::env::var(key).unwrap_or_default();

    if env("WAYLAND_DISPLAY").is_empty() {
        report.push(Status::Fail, "wayland session", "WAYLAND_DISPLAY is unset; this is not a Wayland session and wallpapers cannot be shown");
    } else {
        report.push(Status::Pass, "wayland session", env("WAYLAND_DISPLAY"));
    }

    let desktop = env("XDG_CURRENT_DESKTOP");
    match layer_shell_support(&desktop) {
        LayerShell::Yes => report.push(Status::Pass, "wlr-layer-shell", format!("{desktop} supports zwlr_layer_shell_v1")),
        LayerShell::No => report.push(Status::Fail, "wlr-layer-shell", format!("{desktop} does not support wlr-layer-shell; the picker can browse but cannot apply wallpapers (set an external apply hook)")),
        LayerShell::Unknown => report.push(Status::Warn, "wlr-layer-shell", format!("unknown compositor '{desktop}'; layer-shell support not verified")),
    }
}

fn check_renderers(report: &mut Report, config: &Config) {
    match skwd_wall_core::infrastructure::paper::PaperClient::configured(config).capabilities() {
        Ok(capabilities) => report_renderer_capabilities(report, &capabilities),
        Err(error) => report.push(
            Status::Fail,
            "skwd-paper renderers",
            format!("runtime renderer capabilities are unavailable: {error:#}"),
        ),
    }
}

fn report_renderer_capabilities(
    report: &mut Report,
    capabilities: &skwd_wall_core::infrastructure::paper::CapabilitiesResult,
) {
    if capabilities.renderers.is_empty() {
        report.push(
            Status::Fail,
            "skwd-paper renderers",
            "Paper did not report runtime renderer capabilities; upgrade skwd-paper",
        );
        return;
    }
    for renderer in &capabilities.renderers {
        let status = if renderer.available() {
            Status::Pass
        } else if renderer.discovery
            == skwd_wall_core::infrastructure::paper::RendererDiscovery::Unresolved
        {
            Status::Warn
        } else {
            Status::Fail
        };
        let path = renderer.path.as_deref().unwrap_or("unresolved");
        let diagnostic = renderer
            .diagnostic
            .as_deref()
            .map_or_else(String::new, |message| format!("; {message}"));
        report.push(
            status,
            &renderer.executable,
            format!(
                "source={}; path={path}; present={}; executable={}{}",
                renderer.discovery.as_str(),
                renderer.present,
                renderer.executable_file,
                diagnostic
            ),
        );
        for dependency in &renderer.dependencies {
            let status = if dependency.available {
                Status::Pass
            } else if renderer.executable_file {
                Status::Fail
            } else {
                Status::Warn
            };
            report.push(
                status,
                &format!("{}/{}", renderer.executable, dependency.name),
                &dependency.detail,
            );
        }
    }
}

fn check_socket(report: &mut Report) {
    let sock = socket_path();
    if UnixStream::connect(&sock).is_ok() {
        report.push(Status::Pass, "daemon socket", sock.display().to_string());
    } else {
        report.push(
            Status::Warn,
            "daemon socket",
            format!("not reachable at {} (daemon not running?)", sock.display()),
        );
    }
}

fn check_config_file(report: &mut Report) {
    let cpath = config_path();
    if !cpath.is_file() {
        report.push(
            Status::Warn,
            "config.json",
            format!("{} not found; built-in defaults are in use", cpath.display()),
        );
        return;
    }
    let parsed = std::fs::read_to_string(&cpath)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
    match parsed {
        Some(_) => report.push(Status::Pass, "config.json", cpath.display().to_string()),
        None => report.push(
            Status::Fail,
            "config.json",
            format!("{} exists but does not parse as JSON; defaults are in use", cpath.display()),
        ),
    }
}

fn check_traps(report: &mut Report, cfg: &Config) {
    if pickonly_trap(cfg.pick_only_mode(), cfg.post_processing().len()) {
        report.push(Status::Warn, "pickOnlyMode", "pickOnlyMode is on with no postProcessing commands; applies will do nothing (the picker hands off to external commands that are not configured)");
    }

    if solar_coords_unset(
        cfg.schedule_enabled(),
        cfg.schedule_solar(),
        cfg.latitude(),
        cfg.longitude(),
    ) {
        report.push(Status::Warn, "solar location", "solar day/night scheduling is enabled but no latitude/longitude is set (both default to 0.0, off the coast of West Africa); sunrise/sunset will fire at equatorial times - set schedule.latitude and schedule.longitude");
    }
}

fn check_wallpaper_dir(report: &mut Report, cfg: &Config) {
    let wdir = cfg.wallpaper_dir();
    if std::path::Path::new(&wdir).is_dir() {
        report.push(Status::Pass, "wallpaper dir", wdir);
    } else {
        report.push(
            Status::Warn,
            "wallpaper dir",
            format!("{wdir} does not exist; the library will be empty"),
        );
    }
}

fn check_cache(report: &mut Report, cfg: &Config) {
    let cache = cfg.cache_dir();
    let probe = std::path::Path::new(&cache).join(".doctor-write-probe");
    match std::fs::write(&probe, b"x") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            report.push(Status::Pass, "cache writable", cache);
        }
        Err(err) => report.push(Status::Fail, "cache writable", format!("{cache}: {err}")),
    }
}

fn check_theme_backend(report: &mut Report, cfg: &Config) {
    let backend = cfg.theme().backend();
    match backend.as_str() {
        "off" => {
            report.push(
                Status::Pass,
                "theme backend",
                "off (no colour scheme is generated on apply)",
            );
        }
        "native" => {
            report.push(
                Status::Pass,
                "theme backend",
                "native (skwd-colour, built in; no dependency)",
            );
        }
        "skwd-iris" | "skwd-pywal" | "skwd-wallust" => {
            report.push(
                Status::Pass,
                "theme backend",
                format!("{backend} (built in; no dependency)"),
            );
        }
        "static" => {
            report.push(Status::Pass, "theme backend", "static (fixed palette; no dependency)");
        }
        cli => {
            let exe = if cli == "pywal" { "wal" } else { cli };
            match find_bin(exe) {
                Some(path) => {
                    report.push(
                        Status::Pass,
                        "theme backend",
                        format!("{cli}: {}", path.display()),
                    );
                }
                None => report.push(
                    Status::Warn,
                    "theme backend",
                    format!("{cli} not on PATH; theming falls back to native"),
                ),
            }
        }
    }
}

fn check_optional_bins(report: &mut Report, cfg: &Config) {
    let optional: &[(&str, bool)] = &[
        ("matugen", cfg.theme().backend() == "matugen"),
        ("ffmpeg", true),
        ("ffprobe", true),
        ("awww", cfg.renderer().engine() == "awww"),
        ("steamcmd", true),
    ];
    for (bin, relevant) in optional {
        if !relevant {
            continue;
        }
        match find_bin(bin) {
            Some(path) => report.push(Status::Pass, bin, path.display().to_string()),
            None => report.push(
                Status::Warn,
                bin,
                "not installed (optional; the feature using it will be unavailable)",
            ),
        }
    }
}

fn check_sibling_bins(report: &mut Report) {
    for (label, why) in [
        ("skwd-wall-scan", "library scanning and native theming stop working"),
        ("skwd-wall-effects", "effects and recolours stop working"),
    ] {
        let path = skwd_wall_core::paths::sibling_bin(label);
        if path.is_file() {
            report.push(Status::Pass, label, path.display().to_string());
        } else {
            report.push(Status::Fail, label, format!("{} not found; {why}", path.display()));
        }
    }
}

pub fn checks_json() -> serde_json::Value {
    build_report().to_json()
}

pub fn run(json: bool) -> i32 {
    let report = build_report();
    if json {
        println!("{}", serde_json::to_string_pretty(&report.to_json()).unwrap_or_default());
    } else {
        println!("skwd-walld doctor\n");
        let width = report.lines.iter().map(|(_, label, _)| label.len()).max().unwrap_or(0);
        for (status, label, detail) in &report.lines {
            println!("  {} {label:<width$}  {detail}", status.marker());
        }
        let fails = report.lines.iter().filter(|(status, ..)| *status == Status::Fail).count();
        let warns = report.lines.iter().filter(|(status, ..)| *status == Status::Warn).count();
        println!("\n{} pass, {warns} warn, {fails} fail", report.lines.len() - warns - fails);
    }
    report.exit_code()
}

fn check_templates(config: &skwd_wall_core::config::Config, report: &mut Report) {
    let dir = config.theme().templates_dir();
    let count = std::fs::read_dir(&dir).map_or(0, |entries| {
        entries.filter_map(Result::ok).filter(|entry| entry.path().is_file()).count()
    });
    let integrations = config.theme().integrations().len();
    if integrations == 0 {
        report.push(Status::Pass, "integration templates", "no integrations configured");
    } else if count == 0 {
        report.push(
            Status::Fail,
            "integration templates",
            format!(
                "{integrations} integrations configured but no templates found in {}",
                dir.display()
            ),
        );
    } else {
        report.push(
            Status::Pass,
            "integration templates",
            format!("{count} templates in {}", dir.display()),
        );
    }
}

pub fn bug_report_to_file() -> Result<std::path::PathBuf, String> {
    use std::fmt::Write as _;
    let report = build_report();
    let mut out = String::new();
    let _ = writeln!(out, "skwd-wall bug report");
    let _ = writeln!(out, "version: {}", skwd_wall_core::version());
    if let Ok(dur) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        let _ = writeln!(out, "generated (unix seconds): {}", dur.as_secs());
    }

    let os = std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|line| line.strip_prefix("PRETTY_NAME="))
                .map(|value| value.trim_matches('"').to_string())
        })
        .unwrap_or_else(|| "unknown".into());
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
    let _ = writeln!(out, "\n== system ==");
    let _ = writeln!(out, "os: {os}");
    let _ = writeln!(out, "kernel: {}", kernel.trim());
    let _ = writeln!(out, "arch: {}", std::env::consts::ARCH);

    let _ = write!(out, "\n== {}", skwd_wall_core::diag::env_report());

    let _ = writeln!(out, "== doctor ==");
    for (status, label, detail) in &report.lines {
        let _ = writeln!(out, "  [{}] {label}: {detail}", status.word());
    }

    let strip_ansi = |line: &str| -> String {
        let mut clean = String::with_capacity(line.len());
        let mut chars = line.chars();
        while let Some(ch) = chars.next() {
            if ch == '\u{1b}' {
                for next in chars.by_ref() {
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                clean.push(ch);
            }
        }
        clean
    };

    let _ = writeln!(out, "\n== logs ==");
    let dir = skwd_wall_core::paths::cache_dir();
    let mut logs: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".log"))
        })
        .collect();
    logs.sort();
    for log in &logs {
        let name = log.file_name().and_then(|name| name.to_str()).unwrap_or("?");
        let content = std::fs::read_to_string(log).unwrap_or_default();
        let mut tail: Vec<&str> = content.lines().rev().take(400).collect();
        tail.reverse();
        let _ = writeln!(out, "\n--- {name} (last {} lines) ---", tail.len());
        for line in tail {
            let _ = writeln!(out, "{}", strip_ansi(line));
        }
    }

    let config = Config::load();
    let out = redact_bug_report(&out, &config);

    let path = dir.join("skwd-wall-report.txt");
    skwd_wall_core::paths::atomic_write_mode(&path, out.as_bytes(), Some(0o600))
        .map_err(|err| format!("failed to write bug report to {}: {err}", path.display()))?;
    Ok(path)
}

fn redact_bug_report(report: &str, config: &Config) -> String {
    let secrets = [
        config.wallhaven_api_key(),
        config.steam_api_key(),
        config.unsplash_access_key(),
        config.pexels_api_key(),
    ];
    let secret_refs: Vec<&str> = secrets
        .iter()
        .map(String::as_str)
        .filter(|secret| !secret.is_empty() && *secret != "DEMO_KEY")
        .collect();
    wall_proto::redact_known_secrets(report, &secret_refs)
}

pub fn bug_report() -> i32 {
    match bug_report_to_file() {
        Ok(path) => {
            println!("bug report written to {}", path.display());
            println!("attach that file (and describe what you did) when you open an issue.");
            0
        }
        Err(err) => {
            eprintln!("{err}");
            1
        }
    }
}

mod tests;
