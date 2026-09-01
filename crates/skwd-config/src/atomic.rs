use std::io::Write;
use std::path::Path;

fn suffix() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!("{}.{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed))
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    atomic_write_mode(path, bytes, None)
}

pub fn atomic_write_mode(path: &Path, bytes: &[u8], mode: Option<u32>) -> std::io::Result<()> {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.tmp", suffix()));
    let temporary = path.with_file_name(name);
    let write = || -> std::io::Result<()> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        configure_mode(&mut options, mode);
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        apply_mode(&file, mode)?;
        file.sync_all()
    };
    if let Err(error) = write() {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    if let Err(error) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    if let Some(directory) = path.parent() {
        let _ = std::fs::File::open(directory).map(|handle| handle.sync_all());
    }
    Ok(())
}

#[cfg(unix)]
fn configure_mode(options: &mut std::fs::OpenOptions, mode: Option<u32>) {
    use std::os::unix::fs::OpenOptionsExt;
    if let Some(bits) = mode {
        options.mode(bits);
    }
}

#[cfg(not(unix))]
fn configure_mode(_options: &mut std::fs::OpenOptions, _mode: Option<u32>) {}

#[cfg(unix)]
fn apply_mode(file: &std::fs::File, mode: Option<u32>) -> std::io::Result<()> {
    if let Some(bits) = mode {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(bits))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn apply_mode(_file: &std::fs::File, _mode: Option<u32>) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
#[path = "atomic_tests.rs"]
mod tests;
