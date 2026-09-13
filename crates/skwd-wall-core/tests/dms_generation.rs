#![cfg(feature = "daemon")]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};
use skwd_wall_core::{config::Config, dms, material, theme, theme_provider};

fn check(root: &Path) {
    let cfg = Config::from_root(json!({
        "paths":{"cache":root.join("skwd")},
        "theme":{"authority":"dms","policy":"wallpaper","mode":"light"},
        "integrations":[{"name":"test","template":root.join("template"),"output":root.join("rendered")}]
    }));
    assert!(theme::apply(&cfg, "first image.png"));
    let scheme = root.join("skwd/scheme.json");
    let document: Value = serde_json::from_slice(&std::fs::read(&scheme).unwrap()).unwrap();
    assert_eq!(document["mode"], "light");
    assert_eq!(document["dank16"]["color1"]["dark"], "#123456");
    assert_eq!(material::role(&document, "primary", "default").unwrap(), "#abcdef");
    assert_eq!(std::fs::read_to_string(root.join("rendered")).unwrap(), "#abcdef #123456");
    assert_eq!(std::fs::read_to_string(root.join("app-hook")).unwrap(), "first image.png");
    let calls = std::fs::read_to_string(root.join("calls")).unwrap();
    assert!(calls.contains("--skip-templates\nkitty,nvim\n"));
    assert!(calls.contains("--run-user-templates=false"));
    assert!(!calls.contains("--dry-run"));
    assert!(theme::apply(&cfg, "same"));
    assert_eq!(std::fs::read(&scheme).unwrap(), document.to_string().as_bytes());
    let before = std::fs::read(&scheme).unwrap();
    assert!(!theme::apply(&cfg, "fail"));
    assert_eq!(std::fs::read(&scheme).unwrap(), before);
    assert!(!theme::apply(&cfg, "incomplete"));
    assert_eq!(std::fs::read(&scheme).unwrap(), before);
    assert!(dms::shell_document(&json!({"colors":{"dark":{},"light":{}}}), true).is_none());
    let mut changed: Value =
        serde_json::from_slice(&std::fs::read(root.join("tokens.json")).unwrap()).unwrap();
    changed["mode"] = json!("dark");
    changed["colors"]["dark"]["primary"] = json!("#998877");
    std::fs::write(root.join("cache/DankMaterialShell/dms-colors.json"), changed.to_string())
        .unwrap();
    assert!(theme_provider::import(&cfg, "dms", false));
    let imported: Value = serde_json::from_slice(&std::fs::read(&scheme).unwrap()).unwrap();
    assert_eq!(imported["mode"], "dark");
    assert_eq!(material::role(&imported, "primary", "default").unwrap(), "#998877");
    assert!(!theme_provider::import(&cfg, "dms", false));
    changed["colors"]["dark"]["primary"] = json!("broken");
    std::fs::write(root.join("cache/DankMaterialShell/dms-colors.json"), changed.to_string())
        .unwrap();
    assert!(!theme_provider::import(&cfg, "dms", false));
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(&scheme).unwrap()).unwrap(),
        imported
    );
}

#[test]
fn dms_generation_applies_apps_and_publishes_complete_palette() {
    if let Ok(root) = std::env::var("SKWD_DMS_TEST_ROOT") {
        check(Path::new(&root));
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for name in [
        "bin",
        "config/DankMaterialShell",
        "config/quickshell/dms/matugen/configs",
        "cache/DankMaterialShell",
        "skwd",
    ] {
        std::fs::create_dir_all(root.join(name)).unwrap();
    }
    std::fs::write(root.join("config/quickshell/dms/matugen/configs/base.toml"), "").unwrap();
    std::fs::write(
        root.join("config/DankMaterialShell/settings.json"),
        json!({"matugenTemplateKitty":false,"runUserMatugenTemplates":false}).to_string(),
    )
    .unwrap();
    std::fs::write(
        root.join("template"),
        "{{colors.primary.default.hex}} {{colors.primary.dark.hex}}",
    )
    .unwrap();
    let document = material::document_with("#abcdef", true, "content").unwrap();
    let mut colors = dms::dank_colors_json(&document).unwrap();
    colors["colors"]["dark"]["primary"] = json!("#123456");
    colors["colors"]["light"]["primary"] = json!("#abcdef");
    colors["dank16"] = json!({"color1":{"dark":"#123456","light":"#abcdef"}});
    std::fs::write(root.join("tokens.json"), colors.to_string()).unwrap();
    for (name, script) in [
        ("matugen", "#!/bin/sh\n[ \"$1\" = --version ]\n"),
        (
            "dms",
            r#"#!/usr/bin/python3
import sys, os, json, pathlib
r=pathlib.Path(os.environ['SKWD_DMS_TEST_ROOT'])
a=sys.argv[1:]
if a == ['version']: print('dms 1.6'); sys.exit(0)
if a == ['matugen','check']: print('[{"id":"kitty"},{"id":"nvim"},{"id":"ghostty"}]'); sys.exit(0)
assert a[:2] == ['matugen','queue'], a
(r/'calls').write_text('\n'.join(a)+'\n')
image=a[a.index('--value')+1]
if image == 'fail': sys.exit(1)
if image == 'same': sys.exit(2)
(r/'app-hook').write_text(image)
(r/'cache/DankMaterialShell/dms-colors.json').write_bytes(b'{}' if image == 'incomplete' else (r/'tokens.json').read_bytes())
"#,
        ),
    ] {
        let path = root.join("bin").join(name);
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let out = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "dms_generation_applies_apps_and_publishes_complete_palette",
            "--nocapture",
        ])
        .env("SKWD_DMS_TEST_ROOT", root)
        .env("HOME", root)
        .env("PATH", format!("{}:/usr/bin", root.join("bin").display()))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_RUNTIME_DIR", root.join("runtime"))
        .env_remove("DMS_SHELL_DIR")
        .env_remove("DMS_DISABLE_MATUGEN")
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
