use std::fmt::Write;
use std::path::Path;

use anyhow::{Result, ensure};
use serde_json::Value;
use skwd_config::keys::lock_screens as keys;

use super::ownership::{self, Patch};

const HYPRLOCK_CONFIG: &str = "hypr/hyprlock.conf";
const SWAYLOCK_CONFIG: &str = "swaylock/config";
const SWAYLOCK_LEGACY_CONFIG: &str = ".swaylock/config";
const HYPRLOCK_RECEIPT: &str = "hyprlock.json";
const SWAYLOCK_RECEIPT: &str = "swaylock.json";
const HYPRLOCK_FRAGMENT: &str = "hyprlock.conf";
const VARIABLE: &str = "$skwd_lock_image";
const BACKGROUND: &str = "background";
const INPUT_FIELD: &str = "input-field";
const PATH_KEY: &str = "path";
const IMAGE_KEY: &str = "image";
const SOURCE_KEY: &str = "source";
const BLOCK_END: &str = "}";
const DEFAULT_BACKGROUND: &str = "background {\n    path = $skwd_lock_image_0\n}\n";
const DEFAULT_INPUT_FIELD: &str = "input-field {\n    size = 400, 90\n    position = 0, 0\n    halign = center\n    valign = center\n}\n";

fn hyprlock(original: &str, fragment: &Path) -> Result<Vec<Patch>> {
    ensure!(!original.contains(VARIABLE), "Existing Skwd Hyprlock variable needs review");
    let mut patches = Vec::new();
    let mut background = None;
    let mut offset = 0;
    let mut rendered = String::new();
    let mut has_path = false;
    let mut found_background = false;
    let mut found_input = false;
    for line in original.split_inclusive('\n') {
        let content = line.split('#').next().unwrap_or_default().trim();
        found_input |=
            content.ends_with('{') && content.trim_end_matches('{').trim() == INPUT_FIELD;
        if content.ends_with('{') && content.trim_end_matches('{').trim() == BACKGROUND {
            ensure!(background.is_none(), "Nested Hyprlock background is unsupported");
            background = Some(offset);
            found_background = true;
            rendered.clear();
            has_path = false;
        }
        if background.is_some() {
            ensure!(
                !content.contains('{') || content.trim_end_matches('{').trim() == BACKGROUND,
                "Nested Hyprlock background is unsupported"
            );
            if content.split_once('=').is_some_and(|(key, _)| key.trim() == PATH_KEY) {
                ensure!(!has_path, "Repeated Hyprlock background path needs review");
                let indent = &line[..line.len() - line.trim_start().len()];
                writeln!(rendered, "{indent}{PATH_KEY} = {VARIABLE}_{}", patches.len())?;
                has_path = true;
            } else {
                rendered.push_str(line);
            }
            if content == BLOCK_END {
                let start = background.take().unwrap();
                if has_path {
                    patches.push(Patch {
                        before: original[start..offset + line.len()].to_owned(),
                        after: rendered.clone(),
                        retained_section: None,
                    });
                } else {
                    let opener = original[start..].split_inclusive('\n').next().unwrap_or_default();
                    patches.push(Patch {
                        before: opener.to_owned(),
                        after: format!("{opener}    {PATH_KEY} = {VARIABLE}_{}\n", patches.len()),
                        retained_section: None,
                    });
                }
            }
        }
        offset += line.len();
    }
    ensure!(background.is_none(), "Unclosed Hyprlock background");
    if !found_background {
        let mut insertion = String::new();
        if !original.is_empty() {
            insertion.push('\n');
        }
        insertion.push_str(DEFAULT_BACKGROUND);
        if !found_input {
            insertion.push_str(DEFAULT_INPUT_FIELD);
        }
        patches.push(Patch { before: String::new(), after: insertion, retained_section: None });
    } else if !found_input {
        patches.push(Patch {
            before: String::new(),
            after: DEFAULT_INPUT_FIELD.to_owned(),
            retained_section: None,
        });
    }
    patches.push(Patch {
        before: String::new(),
        after: format!("{SOURCE_KEY} = {}\n", fragment.display()),
        retained_section: None,
    });
    Ok(patches)
}

pub(super) fn swaylock(original: &str, image: &Path) -> Result<Vec<Patch>> {
    let mut patches = Vec::new();
    for line in original.split_inclusive('\n') {
        let content = line.trim();
        if content.split_once('=').is_some_and(|(key, _)| key.trim() == IMAGE_KEY) {
            patches.push(Patch {
                before: line.to_owned(),
                after: String::new(),
                retained_section: None,
            });
        }
    }
    ensure!(patches.len() <= 1, "Multiple Swaylock image overrides need manual setup");
    let directive = format!("{IMAGE_KEY}={}\n", image.display());
    if let Some(patch) = patches.first_mut() {
        patch.after = directive;
    } else {
        patches.push(Patch { before: String::new(), after: directive, retained_section: None });
    }
    Ok(patches)
}

pub(super) fn sway_path(config_home: &Path, home: &Path) -> std::path::PathBuf {
    let legacy = home.join(SWAYLOCK_LEGACY_CONFIG);
    if std::fs::symlink_metadata(&legacy).is_ok() {
        legacy
    } else {
        config_home.join(SWAYLOCK_CONFIG)
    }
}

pub(super) fn sync(
    config: &Value,
    image: Option<&Path>,
    root: &Path,
    config_home: &Path,
) -> Result<()> {
    let hypr_result = sync_hyprlock(config, image, root, config_home);
    let sway_result = sync_swaylock(config, image, root, config_home);
    let errors: Vec<_> = [hypr_result, sway_result]
        .into_iter()
        .filter_map(Result::err)
        .map(|error| error.to_string())
        .collect();
    ensure!(errors.is_empty(), "{}", errors.join(super::ERROR_SEPARATOR));
    Ok(())
}

pub(super) fn sync_hyprlock(
    config: &Value,
    image: Option<&Path>,
    root: &Path,
    config_home: &Path,
) -> Result<()> {
    let hypr =
        super::enabled(config) && skwd_config::bool_false_unless_true(config, keys::HYPRLOCK);
    if hypr && image.is_none() {
        return Ok(());
    }
    let image = root.join(super::IMAGE_FILE);
    let fragment = root.join(HYPRLOCK_FRAGMENT);
    if hypr {
        ensure!(
            !image.to_string_lossy().contains(['\n', '\r', '#', '$']),
            "Unsupported Hyprlock image path"
        );
    }
    ownership::manage(
        &config_home.join(HYPRLOCK_CONFIG),
        &root.join(HYPRLOCK_RECEIPT),
        hypr,
        |text| {
            let patches = hyprlock(text, &fragment)?;
            let mut variables = String::new();
            for index in 0..patches.iter().filter(|patch| patch.after.contains(VARIABLE)).count() {
                writeln!(variables, "{VARIABLE}_{index} = {}", image.display())?;
            }
            ownership::write(&fragment, &variables)?;
            Ok(patches)
        },
    )
}

pub(super) fn sync_swaylock(
    config: &Value,
    image: Option<&Path>,
    root: &Path,
    config_home: &Path,
) -> Result<()> {
    let sway =
        super::enabled(config) && skwd_config::bool_false_unless_true(config, keys::SWAYLOCK);
    if sway && image.is_none() {
        return Ok(());
    }
    let image = root.join(super::IMAGE_FILE);
    let sway_config = sway_path(config_home, Path::new(&skwd_config::home()));
    ownership::manage(&sway_config, &root.join(SWAYLOCK_RECEIPT), sway, |text| {
        swaylock(text, &image)
    })
}
