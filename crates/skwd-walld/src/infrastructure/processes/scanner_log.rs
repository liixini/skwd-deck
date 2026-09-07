use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub(super) fn attach(command: &mut std::process::Command, path: PathBuf) -> std::io::Result<()> {
    let (mut reader, writer) = std::io::pipe()?;
    command.stdout(writer.try_clone()?).stderr(writer);
    std::thread::Builder::new().name("scanner-log".into()).spawn(move || {
        if let Err(error) = drain(&mut reader, &path) {
            log::warn!("scanner log capture failed: {error}");
            let _ = std::io::copy(&mut reader, &mut std::io::sink());
        }
    })?;
    Ok(())
}

fn drain(mut reader: impl Read, path: &Path) -> std::io::Result<()> {
    let mut writer = skwd_log::RotatingWriter::new(path)?;
    let mut buffer = [0u8; 8192];
    loop {
        let length = match reader.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(length) => length,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        writer.write_all(&buffer[..length])?;
    }
}

#[cfg(test)]
mod tests;
