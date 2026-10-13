use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use super::super::{IMAGE_FILE, ownership};
use super::{settings, storage};

pub use wall_proto::helm::{LOCKSCREEN_CHOICES as CHOICES, LOCKSCREEN_SYSTEM as VERB};
const ENABLE: &str = "enable";
const DISABLE: &str = "disable";
const CALLER_ENV: [&str; 2] = ["PKEXEC_UID", "SUDO_UID"];
pub(super) const SYSTEM_ROOT: &str = "/";
const DATA: &str = "var/lib/skwd/lockscreens";
const RECEIPT: &str = "sddm.json";
const PATCHES: &str = "sddm-patches.json";
const SELECTION_PATCHES: &str = "sddm-selection.json";
const VERSION: u32 = 1;

pub(super) struct Paths {
    pub(super) root: PathBuf,
    pub(super) owner: u32,
}

#[derive(Deserialize, Serialize)]
struct Receipt {
    version: u32,
    uid: u32,
    config: PathBuf,
    selected: Option<String>,
    config_mode: u32,
    system_mode: u32,
}

pub use wall_proto::SddmChoices as Selection;

pub fn image_for_user(uid: u32) -> PathBuf {
    PathBuf::from(SYSTEM_ROOT).join(DATA).join(uid.to_string()).join(IMAGE_FILE)
}

pub fn choices() -> Result<Selection> {
    settings::choices(&Paths { root: PathBuf::from(SYSTEM_ROOT), owner: 0 })
}

pub fn run(args: &[String]) -> Result<PathBuf> {
    let (action, selected) = match args {
        [action] => (action.as_str(), None),
        [action, name] if action == ENABLE => (action.as_str(), Some(name.as_str())),
        _ => anyhow::bail!("Expected lockscreen-system enable [theme] or disable"),
    };
    ensure!(matches!(action, ENABLE | DISABLE), "Unknown lockscreen-system action");
    ensure!(unsafe { libc::geteuid() } == 0, "SDDM setup requires administrator authorization");
    let caller = CALLER_ENV.iter().find_map(|name| std::env::var(name).ok());
    let uid: u32 = caller.context("Missing authenticated administrator caller")?.parse()?;
    ensure!(uid != 0, "SDDM wallpaper requires a desktop user");
    sync(&Paths { root: PathBuf::from(SYSTEM_ROOT), owner: 0 }, uid, action == ENABLE, selected)
}

#[expect(clippy::too_many_lines, reason = "Privileged SDDM configuration transaction")]
fn sync(paths: &Paths, uid: u32, enabled: bool, selected: Option<&str>) -> Result<PathBuf> {
    let data = paths.root.join(DATA);
    if !enabled && !data.exists() {
        return Ok(data.join(uid.to_string()).join(IMAGE_FILE));
    }
    storage::directory(paths, &data)?;
    let _guard = storage::lock(paths, &data)?;
    let record = data.join(RECEIPT);
    let patches = data.join(PATCHES);
    let selection_patches = data.join(SELECTION_PATCHES);
    let system_config = paths.root.join(settings::CONFIG);
    storage::trusted(paths, &selection_patches)?;
    storage::trusted(paths, &system_config)?;
    storage::trusted(paths, &record)?;
    storage::trusted(paths, &patches)?;
    storage::trusted(paths, &super::super::backup::marker(&patches))?;
    storage::trusted(paths, &super::super::backup::marker(&selection_patches))?;
    let image_dir = data.join(uid.to_string());
    let image = image_dir.join(IMAGE_FILE);
    let saved =
        ownership::read(&record)?.map(|text| serde_json::from_str::<Receipt>(&text)).transpose()?;
    let directory_record = data.join(format!("{uid}.json"));
    storage::trusted(paths, &directory_record)?;
    if let Some(receipt) = &saved {
        ensure!(
            receipt.version == VERSION && receipt.uid == uid,
            "SDDM belongs to another user or receipt version"
        );
        storage::trusted(paths, &receipt.config)?;
        ensure!(receipt.config.starts_with(&paths.root), "Invalid SDDM receipt location");
    }
    if !enabled {
        if let Some(receipt) = saved {
            super::super::backup::prepare(&receipt.config, &patches)?;
            if receipt.selected.is_some() {
                super::super::backup::prepare(&system_config, &selection_patches)?;
            }
            ownership::manage(&receipt.config, &patches, false, |_| unreachable!())?;
            if receipt.selected.is_some() {
                ownership::manage(&system_config, &selection_patches, false, |_| unreachable!())?;
                storage::restore_mode(&system_config, receipt.system_mode)?;
            }
            storage::restore_mode(&receipt.config, receipt.config_mode)?;
            std::fs::remove_file(record)?;
        }
        return Ok(image);
    }
    let (current, _) = settings::selection(paths)?;
    if let Some(name) = selected {
        ensure!(
            current.is_empty() || current == name,
            "The selected SDDM theme differs from the active theme"
        );
    }
    let name = selected.unwrap_or(&current);
    let config = settings::theme(paths, name)?;
    if let Some(receipt) = &saved {
        ensure!(receipt.config == config, "SDDM theme changed outside Skwd");
    } else {
        if image_dir.exists() {
            ensure!(
                ownership::read(&directory_record)?.as_deref() == Some(&uid.to_string()),
                "Unowned SDDM export directory already exists"
            );
        }
        let receipt = Receipt {
            version: VERSION,
            uid,
            config: config.clone(),
            selected: selected.map(str::to_owned),
            config_mode: storage::mode(&config)?,
            system_mode: storage::mode(&system_config)?,
        };
        ownership::write(&record, &serde_json::to_string(&receipt)?)?;
    }
    let result = (|| {
        super::super::backup::prepare(&config, &patches)?;
        if selected.is_some() || saved.as_ref().is_some_and(|record| record.selected.is_some()) {
            super::super::backup::prepare(&system_config, &selection_patches)?;
        }
        ownership::write(&directory_record, &uid.to_string())?;
        storage::export_directory(paths, &image_dir, uid)?;
        ownership::manage(&config, &patches, true, |text| settings::background(text, &image))?;
        storage::readable(&config)?;
        if selected.is_some() || saved.as_ref().is_some_and(|record| record.selected.is_some()) {
            ownership::manage(&system_config, &selection_patches, true, |text| {
                settings::current(text, name)
            })?;
            storage::readable(&system_config)?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        if ownership::manage(&system_config, &selection_patches, false, |_| unreachable!()).is_ok()
            && ownership::manage(&config, &patches, false, |_| unreachable!()).is_ok()
        {
            let receipt: Receipt = serde_json::from_str(&std::fs::read_to_string(&record)?)?;
            storage::restore_mode(&config, receipt.config_mode)?;
            if receipt.selected.is_some() {
                storage::restore_mode(&system_config, receipt.system_mode)?;
            }
            std::fs::remove_file(&record)?;
            let _ = std::fs::remove_dir(&image_dir);
        }
        return Err(error);
    }
    Ok(image)
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
