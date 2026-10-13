use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Cursor;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use skwd_config::keys::lock_screens as keys;
use skwd_wall_core::infrastructure::{lock_screens, plasma::LockScreenCurrent};

const PNG_EXTENSION: &str = "png";
const IMAGE_MODE: u32 = 0o644;
const WORLD_SEARCH: u32 = 0o001;
const LAST_IMAGE: &str = "current";
const NAME_PREFIX: &str = "wallpaper-";
const RECEIPT_EXTENSION: &str = "json";
const HASH_LENGTH: usize = 16;
const SDDM_RECEIPT_PREFIX: &str = "sddm-";
const RECEIPT_VERSION: u32 = 1;
const RECEIPT_MODE: u32 = 0o600;

#[derive(Deserialize, Serialize)]
struct SddmReceipt {
    version: u32,
    path: PathBuf,
    image: Option<PathBuf>,
    pending: Option<PathBuf>,
}

pub(super) struct SddmSnapshot {
    path: PathBuf,
    record: PathBuf,
    image: Option<Vec<u8>>,
    receipt: Option<Vec<u8>>,
}

fn optional_bytes(path: &Path) -> Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn sddm_record(path: &Path, root: &Path) -> PathBuf {
    let mut hash = DefaultHasher::new();
    path.hash(&mut hash);
    root.join(format!("{SDDM_RECEIPT_PREFIX}{:016x}.{RECEIPT_EXTENSION}", hash.finish()))
}

pub(super) fn sddm_snapshot(config: &Value, root: &Path) -> Result<SddmSnapshot> {
    let destination = skwd_config::str_ref(config, keys::SDDM_IMAGE, "");
    let path = PathBuf::from(skwd_config::resolve(destination));
    let record = sddm_record(&path, root);
    lock_screens::check_path(&path)?;
    lock_screens::check_path(&record)?;
    Ok(SddmSnapshot {
        image: optional_bytes(&path)?,
        receipt: optional_bytes(&record)?,
        path,
        record,
    })
}

impl SddmSnapshot {
    pub(super) fn restore(&self, root: &Path) -> Result<()> {
        let installed = optional_bytes(&root.join(lock_screens::IMAGE_FILE))?;
        let current = optional_bytes(&self.path)?;
        let current_receipt = optional_bytes(&self.record)?;
        ensure!(
            current == self.image
                || current == installed
                || (current.is_none() && current_receipt.is_none() && self.receipt.is_some()),
            "SDDM image changed outside Skwd"
        );
        if current != self.image {
            match &self.image {
                Some(bytes) => skwd_config::atomic_write_mode(&self.path, bytes, Some(IMAGE_MODE))?,
                None => std::fs::remove_file(&self.path)?,
            }
        }
        if current_receipt != self.receipt {
            match &self.receipt {
                Some(bytes) => {
                    skwd_config::atomic_write_mode(&self.record, bytes, Some(RECEIPT_MODE))?;
                }
                None => std::fs::remove_file(&self.record)?,
            }
        }
        Ok(())
    }
}

pub(super) fn remove_sddm(config: &Value, root: &Path) -> Result<()> {
    let destination = skwd_config::str_ref(config, keys::SDDM_IMAGE, "").trim();
    if destination.is_empty() {
        return Ok(());
    }
    let path = PathBuf::from(skwd_config::resolve(destination));
    let record = sddm_record(&path, root);
    lock_screens::check_path(&path)?;
    lock_screens::check_path(&record)?;
    let Some(bytes) = optional_bytes(&record)? else {
        return Ok(());
    };
    let receipt: SddmReceipt = serde_json::from_slice(&bytes)?;
    ensure!(
        receipt.version == RECEIPT_VERSION && receipt.path == path,
        "Invalid SDDM export receipt"
    );
    if let Some(current) = optional_bytes(&path)? {
        let owned = [&receipt.image, &receipt.pending].into_iter().flatten().any(|source| {
            lock_screens::check_path(source).is_ok()
                && std::fs::read(source).is_ok_and(|bytes| bytes == current)
        });
        ensure!(owned, "SDDM image changed outside Skwd");
        std::fs::remove_file(path)?;
    }
    std::fs::remove_file(record)?;
    Ok(())
}

fn source(config: &Value, current: Option<&LockScreenCurrent<'_>>) -> Result<PathBuf> {
    if skwd_config::str_ref(config, keys::MODE, keys::OFF) == keys::STATIC {
        let path = skwd_config::str_ref(config, keys::IMAGE, "").trim();
        ensure!(!path.is_empty(), "Fixed lockscreen image is empty");
        return Ok(PathBuf::from(skwd_config::resolve(path)));
    }
    let current = current.context("No applied wallpaper for lockscreen")?;
    let path = if current.kind == wall_proto::kind::STATIC { current.path } else { current.poster };
    ensure!(!path.is_empty(), "Dynamic wallpaper has no static poster");
    ensure!(
        current.kind == wall_proto::kind::STATIC || path != current.path,
        "Dynamic wallpaper has no static poster"
    );
    Ok(PathBuf::from(path))
}

pub(super) fn previous(root: &Path) -> Result<Option<PathBuf>> {
    let file = root.join(LAST_IMAGE);
    lock_screens::check_path(&file)?;
    let name = match std::fs::read_to_string(file) {
        Ok(name) => name,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        name.starts_with(NAME_PREFIX) && Path::new(&name).components().count() == 1,
        "Invalid saved lockscreen image"
    );
    let image = root.join(name);
    lock_screens::check_path(&image)?;
    Ok(image.is_file().then_some(image))
}

pub(super) fn restore(root: &Path, previous: Option<&Path>) -> Result<()> {
    let stable = root.join(lock_screens::IMAGE_FILE);
    let latest = root.join(LAST_IMAGE);
    lock_screens::check_path(&stable)?;
    lock_screens::check_path(&latest)?;
    if let Some(previous) = previous {
        let name = previous.file_name().context("Missing previous lockscreen image name")?;
        skwd_config::atomic_write_mode(&stable, &std::fs::read(previous)?, Some(IMAGE_MODE))?;
        skwd_config::atomic_write(&latest, name.as_encoded_bytes())?;
    } else {
        for file in [&stable, &latest] {
            if file.exists() {
                std::fs::remove_file(file)?;
            }
        }
    }
    Ok(())
}

pub(super) fn render(
    config: &Value,
    current: Option<&LockScreenCurrent<'_>>,
    root: &Path,
) -> Result<PathBuf> {
    let source = source(config, current)?;
    let image = image::ImageReader::open(&source)?
        .with_guessed_format()?
        .decode()
        .with_context(|| format!("Decode lockscreen still {}", source.display()))?;
    let mut png = Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png)?;
    let bytes = png.into_inner();
    let mut hash = DefaultHasher::new();
    bytes.hash(&mut hash);
    let name = format!("{NAME_PREFIX}{:016x}.{PNG_EXTENSION}", hash.finish());
    let path = root.join(&name);
    lock_screens::check_path(&path)?;
    std::fs::create_dir_all(root)?;
    if path.exists() {
        ensure!(std::fs::read(&path)? == bytes, "Lockscreen image identity collision");
    } else {
        skwd_config::atomic_write_mode(&path, &bytes, Some(IMAGE_MODE))?;
    }
    let stable = root.join(lock_screens::IMAGE_FILE);
    lock_screens::check_path(&stable)?;
    skwd_config::atomic_write_mode(&stable, &bytes, Some(IMAGE_MODE))?;
    let latest = root.join(LAST_IMAGE);
    lock_screens::check_path(&latest)?;
    skwd_config::atomic_write(&latest, name.as_bytes())?;
    Ok(path)
}

pub(super) fn export_sddm(config: &Value, image: &Path, root: &Path) -> Result<()> {
    let destination = skwd_config::str_ref(config, keys::SDDM_IMAGE, "").trim();
    if destination.is_empty() {
        return Ok(());
    }
    let path = PathBuf::from(skwd_config::resolve(destination));
    lock_screens::check_path(&path)?;
    ensure!(
        path.extension().is_some_and(|ext| ext == PNG_EXTENSION),
        "SDDM export must use a .png filename"
    );
    for directory in path.parent().context("Missing SDDM directory")?.ancestors() {
        let metadata = std::fs::metadata(directory).with_context(|| {
            format!("SDDM directory must already exist: {}", directory.display())
        })?;
        ensure!(
            metadata.is_dir() && metadata.permissions().mode() & WORLD_SEARCH != 0,
            "SDDM directory must be world-searchable: {}",
            directory.display()
        );
    }
    write_sddm(&path, image, root)?;
    Ok(())
}

fn write_sddm(path: &Path, image: &Path, root: &Path) -> Result<()> {
    let record = sddm_record(path, root);
    lock_screens::check_path(&record)?;
    let mut receipt = match std::fs::read(&record) {
        Ok(bytes) => {
            let receipt: SddmReceipt = serde_json::from_slice(&bytes)?;
            ensure!(
                receipt.version == RECEIPT_VERSION && receipt.path == path,
                "Invalid SDDM export receipt"
            );
            receipt
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            ensure!(!path.exists(), "SDDM destination already exists and is not owned by Skwd");
            SddmReceipt {
                version: RECEIPT_VERSION,
                path: path.to_path_buf(),
                image: None,
                pending: None,
            }
        }
        Err(error) => return Err(error.into()),
    };
    if path.exists() {
        ensure!(!std::fs::metadata(path)?.permissions().readonly(), "SDDM image is read-only");
        let current = std::fs::read(path)?;
        let owned = [&receipt.image, &receipt.pending].into_iter().flatten().any(|source| {
            lock_screens::check_path(source).is_ok()
                && std::fs::read(source).is_ok_and(|bytes| bytes == current)
        });
        ensure!(owned, "SDDM image changed outside Skwd");
    }
    receipt.pending = Some(image.to_path_buf());
    skwd_config::atomic_write_mode(&record, &serde_json::to_vec(&receipt)?, Some(RECEIPT_MODE))?;
    skwd_config::atomic_write_mode(path, &std::fs::read(image)?, Some(IMAGE_MODE))?;
    receipt.image = receipt.pending.take();
    skwd_config::atomic_write_mode(&record, &serde_json::to_vec(&receipt)?, Some(RECEIPT_MODE))?;
    Ok(())
}

pub(super) fn prune(root: &Path, current: &Path, previous: Option<&Path>) -> Result<()> {
    lock_screens::check_path(root)?;
    let files = std::fs::read_dir(root)?.collect::<std::io::Result<Vec<_>>>()?;
    let mut retained = vec![current.to_path_buf()];
    retained.extend(previous.map(Path::to_path_buf));
    for file in &files {
        if file.path().extension().is_some_and(|ext| ext == RECEIPT_EXTENSION) {
            lock_screens::check_path(&file.path())?;
            let receipt: Value = serde_json::from_slice(&std::fs::read(file.path())?)?;
            if let Some(fields) = receipt.as_object() {
                retained.extend(fields.values().filter_map(Value::as_str).map(PathBuf::from));
            }
        }
    }
    for file in files {
        let path = file.path();
        let owned = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|stem| stem.strip_prefix(NAME_PREFIX))
            .is_some_and(|hash| {
                hash.len() == HASH_LENGTH && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            });
        if owned
            && path.extension().is_some_and(|ext| ext == PNG_EXTENSION)
            && file.file_type()?.is_file()
            && !retained.contains(&path)
        {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "export_tests.rs"]
mod tests;
