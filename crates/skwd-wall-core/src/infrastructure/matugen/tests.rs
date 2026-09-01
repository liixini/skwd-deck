#![cfg(test)]

use super::*;
use serde_json::json;

#[test]
fn detached_hook_nonblocking() {
    let start = std::time::Instant::now();
    run_sh_detached("sleep 30");
    assert!(start.elapsed() < std::time::Duration::from_secs(3));
}

#[test]
fn config_skips_missing() {
    let dir = tempfile::tempdir().unwrap();
    let tdir = dir.path().join("templates");
    std::fs::create_dir_all(&tdir).unwrap();
    std::fs::write(tdir.join("good.conf"), "x={{colors.primary.default.hex}}").unwrap();

    let cfg = Config::from_root(json!({
        "paths": { "cache": dir.path().to_str().unwrap(), "templates": tdir.to_str().unwrap() },
        "integrations": [
            { "template": "good.conf", "output": "colors.json" },
            { "template": "missing.conf", "output": "never.json" },
            { "template": "", "output": "skip.json" },
        ]
    }));

    let path = generate_config(&cfg);
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("[templates.integration_0]"));
    assert!(content.contains("good.conf"));
    assert!(content.contains("colors.json"));
    assert!(!content.contains("[templates.integration_1]"));
    assert!(!content.contains("never.json"));
    assert!(!content.contains("[templates.integration_2]"));
}

#[test]
fn empty_templates_table() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = Config::from_root(json!({ "paths": { "cache": dir.path().to_str().unwrap() } }));
    let path = generate_config(&cfg);
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("[templates]"));
}

#[test]
fn auto_never_reaches_cli() {
    let cfg = Config::from_root(serde_json::json!({ "matugen": { "mode": "auto" } }));
    let resolved = super::resolve_cli_mode(&cfg, "/definitely/not/an/image.png", "auto");
    assert!(resolved == "dark" || resolved == "light", "{resolved}");
    assert_eq!(super::resolve_cli_mode(&cfg, "/x.png", "light"), "light");
    assert_eq!(super::resolve_cli_mode(&cfg, "/x.png", "dark"), "dark");
}
