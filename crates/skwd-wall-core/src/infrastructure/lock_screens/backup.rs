use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use super::ownership;

const COPY_SUFFIX: &str = ".skwd-backup";
const RECORD_SUFFIX: &str = ".backup";
const PRIVATE_MODE: u32 = 0o600;

#[derive(Deserialize, Serialize)]
struct Record {
    source: PathBuf,
    copy: Option<PathBuf>,
}

pub(super) fn marker(record: &Path) -> PathBuf {
    suffixed(record, RECORD_SUFFIX)
}

fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

pub fn begin(source: &Path, record: &Path) -> Result<()> {
    ownership::check(source)?;
    let marker = marker(record);
    ownership::check(&marker)?;
    if let Some(file) = open(&marker)? {
        let saved: Record = serde_json::from_reader(file)?;
        ensure!(saved.source == source, "Lockscreen backup source changed");
        if let Some(copy) = saved.copy {
            ownership::check(&copy)?;
            ensure!(open(&copy)?.is_some(), "Lockscreen backup missing");
        }
        return Ok(());
    }
    std::fs::create_dir_all(marker.parent().context("Missing backup record parent")?)?;
    let copy = open(source)?.map(|mut original| capture(source, &mut original)).transpose()?;
    let saved = Record { source: source.to_owned(), copy };
    let result = persist(&marker, &serde_json::to_vec(&saved)?);
    if result.is_err()
        && let Some(copy) = saved.copy
    {
        let _ = std::fs::remove_file(copy);
    }
    result
}

pub(super) fn prepare(source: &Path, record: &Path) -> Result<()> {
    begin(source, record)?;
    if ownership::read(record)?.is_some() {
        return Ok(());
    }
    let saved: Record =
        serde_json::from_reader(open(&marker(record))?.context("Missing backup record")?)?;
    let original = saved.copy.as_deref().map(contents).transpose()?.flatten();
    if contents(source)? != original {
        end(record)?;
        begin(source, record)?;
    }
    Ok(())
}

fn contents(path: &Path) -> Result<Option<Vec<u8>>> {
    ownership::check(path)?;
    open(path)?
        .map(|mut file| {
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            Ok(bytes)
        })
        .transpose()
}

pub fn end(record: &Path) -> Result<()> {
    let marker = marker(record);
    ownership::check(&marker)?;
    match std::fs::remove_file(&marker) {
        Ok(()) => {
            File::open(marker.parent().context("Missing backup record parent")?)?.sync_all()?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn open(path: &Path) -> Result<Option<File>> {
    match OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(path)
    {
        Ok(file) => {
            ensure!(file.metadata()?.is_file(), "Backup source is not a regular file");
            Ok(Some(file))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("Read backup source {}", path.display())),
    }
}

fn create(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(PRIVATE_MODE)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}

fn capture(source: &Path, original: &mut File) -> Result<PathBuf> {
    ensure!(
        !original.metadata()?.permissions().readonly(),
        "Config is read-only: {}",
        source.display()
    );
    let base = suffixed(source, COPY_SUFFIX);
    let mut copy = base.clone();
    let mut index = 0_u64;
    loop {
        match create(&copy) {
            Ok(mut file) => {
                let result = (|| -> Result<()> {
                    std::io::copy(original, &mut file)?;
                    file.sync_all()?;
                    File::open(copy.parent().context("Missing backup parent")?)?.sync_all()?;
                    Ok(())
                })();
                if let Err(error) = result {
                    let _ = std::fs::remove_file(&copy);
                    return Err(error);
                }
                return Ok(copy);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                index = index.checked_add(1).context("Backup names exhausted")?;
                copy = suffixed(&base, &format!(".{index}"));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn persist(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = create(path)?;
    let result = (|| -> Result<()> {
        file.write_all(bytes)?;
        file.sync_all()?;
        File::open(path.parent().context("Missing backup record parent")?)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(path);
    }
    result
}

#[cfg(test)]
#[path = "backup_tests.rs"]
mod tests;
