#![cfg(feature = "daemon")]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};
use skwd_wall_core::{config::Config, material, static_templates, theme};

const TEMPLATE: &str = "{{colors.on_primary.default.hex}} {{colors.on_surface.default.hex}} {{colors.surface_variant.default.hex}} {{colors.surface_container.default.hex}}";

fn config(root: &Path, backend: &str, mode: &str) -> Config {
    Config::from_root(json!({
        "paths": {"cache": root.join("cache"), "noctaliaBin": root.join("bin/noctalia")},
        "theme": {"backend": backend, "authority": backend, "mode": mode,
                  "noctaliaScheme": "muted"}
    }))
}

fn check(root: &Path) {
    let tokens: Value =
        serde_json::from_slice(&std::fs::read(root.join("tokens.json")).unwrap()).unwrap();
    for backend in ["noctalia", "dms"] {
        for (mode, dark) in [("dark", true), ("light", false)] {
            let palette =
                theme::preview_palette(&config(root, backend, mode), "wallpaper.png").unwrap();
            let doc = material::document_from_modes(&tokens, dark).unwrap();
            let expected = material::ui_palette(&doc).unwrap();
            for key in material::UI_KEYS {
                assert_eq!(palette.get(key), expected.get(key), "{backend} {mode} {key}");
            }
            assert_eq!(theme::swatch_from_palette(&palette), theme::swatch_from_palette(&expected));
            assert_eq!(
                static_templates::render(TEMPLATE, &static_templates::material_map(&palette, dark)),
                static_templates::render(
                    TEMPLATE,
                    &static_templates::material_map(&expected, dark)
                )
            );
        }
    }
    let calls = std::fs::read_to_string(root.join("calls")).unwrap();
    assert!(calls.contains("theme wallpaper.png --scheme muted --dark"));
    assert!(calls.contains("theme wallpaper.png --scheme muted --light"));
}

fn executable(path: &Path, script: &str) {
    std::fs::write(path, script).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn previews_match_roles() {
    if let Ok(root) = std::env::var("SKWD_PREVIEW_CASING_ROOT") {
        check(Path::new(&root));
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir(root.join("bin")).unwrap();
    std::fs::create_dir(root.join("cache")).unwrap();
    let doc = material::document_with("#854cff", true, "content").unwrap();
    let mut tokens = json!({"dark": {}, "light": {}});
    let mut nested = json!({"colors": {}});
    for mode in ["dark", "light"] {
        let mut flat = json!({});
        for key in material::ROLE_KEYS {
            let hex = material::role(&doc, key, mode).unwrap();
            tokens[mode][key] = json!(hex);
            nested["colors"][key][mode]["color"] = json!(hex);
            flat[key] = json!(hex);
        }
        std::fs::write(root.join(format!("{mode}.json")), flat.to_string()).unwrap();
    }
    std::fs::write(root.join("tokens.json"), tokens.to_string()).unwrap();
    std::fs::write(root.join("nested.json"), nested.to_string()).unwrap();
    executable(
        &root.join("bin/noctalia"),
        "#!/bin/sh\nif [ \"$1\" = --version ]; then exit 0; fi\nprintf '%s\\n' \"$*\" >> \"$SKWD_PREVIEW_CASING_ROOT/calls\"\ncase \"$*\" in *--dark*) cat \"$SKWD_PREVIEW_CASING_ROOT/dark.json\";; *) cat \"$SKWD_PREVIEW_CASING_ROOT/light.json\";; esac\n",
    );
    executable(&root.join("bin/dms"), "#!/bin/sh\n[ \"$1\" = version ]\n");
    executable(
        &root.join("bin/matugen"),
        "#!/bin/sh\nif [ \"$1\" = --version ]; then exit 0; fi\ncat \"$SKWD_PREVIEW_CASING_ROOT/nested.json\"\n",
    );
    let out = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "previews_match_roles"])
        .env("SKWD_PREVIEW_CASING_ROOT", root)
        .env("PATH", format!("{}:/usr/bin", root.join("bin").display()))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env_remove("SKWD_WALL_V2_CONFIG")
        .env_remove("SKWD_WALL_V2_CACHE")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
}
