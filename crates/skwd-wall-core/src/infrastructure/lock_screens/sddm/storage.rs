use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;

use anyhow::{Result, ensure};

use super::Paths;

const WRITE_MASK: u32 = 0o022;
const DIRECTORY_MODE: u32 = 0o755;
const FILE_MODE: u32 = 0o644;
const KEEP_GROUP: u32 = u32::MAX;
const LOCK_MODE: u32 = 0o600;
const LOCK: &str = "sddm.lock";

pub(super) fn trusted(paths: &Paths, path: &Path) -> Result<()> {
    ensure!(path.starts_with(&paths.root), "SDDM path escapes system root");
    for ancestor in path.ancestors().take_while(|path| path.starts_with(&paths.root)) {
        match std::fs::symlink_metadata(ancestor) {
            Ok(meta) => {
                ensure!(
                    !meta.file_type().is_symlink(),
                    "SDDM symlink path: {}",
                    ancestor.display()
                );
                ensure!(
                    meta.uid() == paths.owner && meta.mode() & WRITE_MASK == 0,
                    "Untrusted SDDM path: {}",
                    ancestor.display()
                );
                ensure!(meta.is_dir() || meta.is_file(), "Unsupported SDDM file type");
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

pub(super) fn directory(paths: &Paths, path: &Path) -> Result<()> {
    trusted(paths, path)?;
    if !path.exists() {
        if let Some(parent) = path.parent() {
            directory(paths, parent)?;
        }
        std::fs::create_dir(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(DIRECTORY_MODE))?;
    }
    ensure!(path.is_dir(), "Expected SDDM directory");
    Ok(())
}

pub(super) fn export_directory(paths: &Paths, path: &Path, uid: u32) -> Result<()> {
    if !path.exists() {
        directory(paths, path)?;
    }
    if std::fs::symlink_metadata(path)?.uid() == paths.owner {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(path)?;
        ensure!(
            unsafe { libc::fchown(file.as_raw_fd(), uid, KEEP_GROUP) } == 0,
            "Cannot assign SDDM export directory: {}",
            std::io::Error::last_os_error()
        );
    }
    let meta = std::fs::symlink_metadata(path)?;
    ensure!(
        meta.is_dir()
            && !meta.file_type().is_symlink()
            && meta.uid() == uid
            && meta.mode() & DIRECTORY_MODE == DIRECTORY_MODE
            && meta.mode() & WRITE_MASK == 0,
        "SDDM export directory ownership changed"
    );
    Ok(())
}

pub(super) fn readable(path: &Path) -> Result<()> {
    let mode = std::fs::metadata(path)?.permissions().mode();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode | FILE_MODE))?;
    Ok(())
}

pub(super) fn lock(paths: &Paths, data: &Path) -> Result<File> {
    let path = data.join(LOCK);
    trusted(paths, &path)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(LOCK_MODE)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    ensure!(
        unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0,
        "Another SDDM setup is running"
    );
    Ok(file)
}

pub(super) fn mode(path: &Path) -> Result<u32> {
    match std::fs::metadata(path) {
        Ok(meta) => Ok(meta.permissions().mode()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(FILE_MODE),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn restore_mode(path: &Path, original: u32) -> Result<()> {
    if path.exists() && mode(path)? == original | FILE_MODE {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(original))?;
    }
    Ok(())
}
