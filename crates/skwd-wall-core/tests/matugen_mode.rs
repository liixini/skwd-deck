#![cfg(feature = "daemon")]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};
use skwd_wall_core::{config::Config, material, matugen, theme};

fn exercise(root: &Path) {
    let base = json!({
        "paths": {"cache": root.join("cache"), "templates": root.join("templates")},
        "theme": {"policy": "wallpaper", "authority": "skwd", "engine": "matugen"},
        "defaultMatugenConfig": root.join("external.toml")
    });
    for (mode, legacy, explicit, expected) in [
        (Some("light"), None, None, "light"),
        (Some("light"), Some("dark"), None, "light"),
        (Some("dark"), Some("light"), None, "dark"),
        (None, Some("light"), None, "light"),
        (None, None, None, "dark"),
        (Some("auto"), Some("dark"), None, "light"),
        (Some("light"), None, Some("dark"), "dark"),
        (Some("smart"), Some("dark"), None, "smart"),
    ] {
        let mut settings = base.clone();
        if let Some(mode) = mode {
            settings["theme"]["mode"] = json!(mode);
        }
        if let Some(legacy) = legacy {
            settings["matugen"]["mode"] = json!(legacy);
        }
        settings["matugen"]["schemeType"] = json!("scheme-smart");
        settings["matugen"]["colorIndex"] = json!(2);
        let config = Config::from_root(settings);
        std::fs::write(root.join("calls"), "").unwrap();
        assert!(matugen::run_with(&config, "a wallpaper's image.png", None, explicit, None));
        let calls = std::fs::read_to_string(root.join("calls")).unwrap();
        let calls: Vec<Value> =
            calls.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
        assert_eq!(calls.len(), 3, "initial attempt, index-zero retry, external templates");
        for call in &calls {
            let args = call.as_array().unwrap();
            let flag = |name: &str| {
                args[args.iter().position(|arg| arg == name).unwrap() + 1].as_str().unwrap()
            };
            assert_eq!(flag("-m"), expected);
            assert_eq!(flag("-t"), "scheme-smart");
            assert!(args.contains(&json!("a wallpaper's image.png")));
        }
        let document: Value =
            serde_json::from_slice(&std::fs::read(root.join("cache/scheme.json")).unwrap())
                .unwrap();
        assert_eq!(document["is_dark_mode"], expected == "dark");
        if mode == Some("smart") {
            assert!(!theme::resolve_dark(&config, "a wallpaper's image.png"));
            let preview = theme::preview_palette(&config, "a wallpaper's image.png").unwrap();
            let applied: Value =
                serde_json::from_slice(&std::fs::read(root.join("cache/colors.json")).unwrap())
                    .unwrap();
            assert_eq!(preview, applied);
        }
    }
}

#[test]
fn matugen_modes_reach_generation_templates_and_palette() {
    if let Ok(root) = std::env::var("SKWD_MATUGEN_TEST_ROOT") {
        exercise(Path::new(&root));
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("bin")).unwrap();
    std::fs::write(root.join("external.toml"), "[config]\n").unwrap();
    let document = material::document_with("#854cff", false, "tonal-spot").unwrap();
    std::fs::write(root.join("document.json"), document.to_string()).unwrap();
    for (name, script) in [
        (
            "matugen",
            r"#!/usr/bin/python3
import json, os, pathlib, sys
root = pathlib.Path(os.environ['SKWD_MATUGEN_TEST_ROOT'])
args = sys.argv[1:]
if args == ['--version']:
    print('matugen 4.2.0')
    sys.exit(0)
with (root / 'calls').open('a') as log:
    log.write(json.dumps(args) + '\n')
if '--dry-run' not in args and args[args.index('--source-color-index') + 1] != '0':
    sys.exit(1)
mode = args[args.index('-m') + 1]
assert mode in ['dark', 'light', 'smart'], mode
assert args[args.index('-t') + 1] == 'scheme-smart'
doc = json.loads((root / 'document.json').read_text())
resolved = 'dark' if mode == 'dark' else 'light'
doc['is_dark_mode'] = resolved == 'dark'
doc['mode'] = resolved
for role in doc['colors'].values():
    role['default'] = role[resolved]
print(json.dumps(doc))
",
        ),
        ("skwd-wall-scan", "#!/bin/sh\nprintf '{\"dark\":false}\\n'\n"),
    ] {
        let path = root.join("bin").join(name);
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let out = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "matugen_modes_reach_generation_templates_and_palette", "--nocapture"])
        .env("SKWD_MATUGEN_TEST_ROOT", root)
        .env("HOME", root)
        .env("PATH", format!("{}:/usr/bin", root.join("bin").display()))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_RUNTIME_DIR", root.join("runtime"))
        .env_remove("SKWD_WALL_V2_CONFIG")
        .env_remove("SKWD_WALL_V2_CACHE")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
