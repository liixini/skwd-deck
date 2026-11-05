use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use skwd_wall_core::config::Config;
use skwd_wall_core::infrastructure::drawing::{self as cache, PACK_RECORD, PackStamp};
use wall_proto::drawing::Device;

const PACK_SETTING: &str = "pack";
pub(super) const RECORD_FORMAT: u32 = 1;
pub(super) const SYSTEM_HOME_ENV: &str = "SKWD_DRAWING_HOME";
const SYSTEM_HOME: &str = "/usr/share/skwd-deck/models/drawing";
pub(super) const SYSTEM_MANIFEST: &str = "pack.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Pack {
    #[serde(default)]
    pub format: u32,
    pub path: PathBuf,
    pub fingerprint: String,
    pub bytes: u64,
    pub inventory: Vec<PackStamp>,
    #[serde(skip)]
    pub device: Device,
}

impl Pack {
    #[cfg(test)]
    pub(super) fn inspect(path: &Path) -> Result<Self> {
        Self::inspect_cancellable(path, &AtomicBool::new(false))
    }

    pub(super) fn inspect_cancellable(path: &Path, cancel: &AtomicBool) -> Result<Self> {
        let mut inventory = vec![PackStamp::capture(path)?];
        let path = path.canonicalize().context("processing manifest is unavailable")?;
        let schema = cache::pack::Pack::read(&path)?;
        let mut fingerprint = std::fs::read(&path)?;
        let mut bytes = 0;
        for file in schema.files() {
            ensure!(!cancel.load(Ordering::Acquire), "processing check cancelled");
            inventory.push(PackStamp::capture(file)?);
            let file = file.canonicalize().context("processing file is missing")?;
            bytes += std::fs::metadata(&file)?.len();
            fingerprint.extend_from_slice(file.as_os_str().as_encoded_bytes());
            fingerprint.extend_from_slice(cache::hash_file_cancellable(&file, cancel)?.as_bytes());
        }
        ensure!(inventory.iter().all(PackStamp::valid), "processing files changed during check");
        Ok(Self {
            format: RECORD_FORMAT,
            path,
            fingerprint: cache::digest(&fingerprint),
            bytes,
            inventory,
            device: schema.device,
        })
    }

    pub(super) fn load(config: &Config) -> Result<Self> {
        let mut pack: Self = cache::read_processing(config)?;
        ensure!(pack.format == RECORD_FORMAT, "processing files need to be registered again");
        ensure!(!pack.inventory.is_empty(), "processing inventory is missing");
        ensure!(pack.inventory.iter().all(PackStamp::valid), "processing files changed");
        pack.device = cache::pack::Pack::read(&pack.path)?.device;
        Ok(pack)
    }

    pub(super) fn save(&self, config: &Config) -> Result<()> {
        self.write_record(config)?;
        save_path(Some(&self.path))
    }

    fn write_record(&self, config: &Config) -> Result<()> {
        let root = cache::root(config);
        std::fs::create_dir_all(&root)?;
        skwd_config::atomic_write(&root.join(PACK_RECORD), &serde_json::to_vec(self)?)?;
        Ok(())
    }
}

pub(super) fn system_manifest(config: &Config) -> Option<PathBuf> {
    let home = std::env::var_os(SYSTEM_HOME_ENV).unwrap_or_else(|| SYSTEM_HOME.into());
    let manifest = PathBuf::from(home).join(SYSTEM_MANIFEST);
    let adoptable = match skwd_config::get(config.root(), skwd_config::keys::drawing::PACK) {
        None => true,
        Some(setting) => setting.as_str().is_some_and(|path| Path::new(path) == manifest),
    };
    (adoptable && manifest.is_file() && Pack::load(config).is_err()).then_some(manifest)
}

pub(super) fn detach(config: &Config) -> Result<()> {
    let path = cache::root(config).join(PACK_RECORD);
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    save_path(None)
}

fn save_path(pack: Option<&Path>) -> Result<()> {
    super::update_config(|drawing| {
        drawing[PACK_SETTING] = serde_json::Value::String(
            pack.map_or_else(String::new, |path| path.to_string_lossy().into_owned()),
        );
    })
}
