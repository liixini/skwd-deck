use skwd_wall_core::config::Config;

pub fn bug_report_to_file() -> Result<std::path::PathBuf, String> {
    use std::fmt::Write as _;
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
