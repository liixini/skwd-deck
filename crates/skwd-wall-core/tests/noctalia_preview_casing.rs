#![cfg(feature = "daemon")]

//! Noctalia's `theme` command emits Material roles in snake_case while the
//! preview palette contract is camelCase. Guards the 4-of-6 swatch regression.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use serde_json::json;
use skwd_wall_core::{config::Config, theme};

/// Exactly what `noctalia theme <img> --scheme <scheme> --dark` prints.
const NOCTALIA_DARK_OUTPUT: &str = r##"{
  "primary": "#854cff",
  "on_primary": "#ffffff",
  "on_surface": "#ffffff",
  "surface": "#121212",
  "surface_variant": "#1e1e1e",
  "surface_container": "#2a2a2a",
  "tertiary": "#a3be8c",
  "background": "#121212",
  "outline": "#5a5a5a"
}"##;

fn config(root: &Path) -> Config {
    Config::from_root(json!({
        "paths": {"cache": root.join("cache"), "noctaliaBin": root.join("noctalia")},
        "theme": {
            "backend": "noctalia", "authority": "noctalia", "mode": "dark",
            "noctaliaScheme": "m3-content"
        }
    }))
}

#[test]
fn noctalia_preview_keeps_six_swatches() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir(root.join("cache")).unwrap();
    std::fs::write(root.join("flat.json"), NOCTALIA_DARK_OUTPUT).unwrap();

    let script = root.join("noctalia");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\n\
             if [ \"$1\" = --version ]; then printf '%s\\n' 'noctalia v5.0.0'; exit 0; fi\n\
             if [ \"$1\" = theme ]; then cat '{root}/flat.json'; fi\n",
            root = root.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

    let palette =
        theme::preview_palette(&config(root), "wallpaper.png").expect("noctalia preview resolves");
    let swatch = theme::swatch_from_palette(&palette);
    assert_eq!(
        swatch.len(),
        6,
        "expected six preview swatches, got {swatch:?} from palette keys {:?}",
        palette.as_object().map(|object| object.keys().collect::<Vec<_>>())
    );
}
