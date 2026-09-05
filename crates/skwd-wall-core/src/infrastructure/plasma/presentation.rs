use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Context;

pub(super) struct Pending {
    files: Vec<(String, PathBuf)>,
}

impl Pending {
    pub(super) fn new(payload: &mut serde_json::Value) -> anyhow::Result<Self> {
        let runtime =
            std::env::var_os("XDG_RUNTIME_DIR").context("Plasma apply needs XDG_RUNTIME_DIR")?;
        Self::create(&PathBuf::from(runtime).join("skwd-paper-plasma"), payload)
    }

    fn create(root: &Path, payload: &mut serde_json::Value) -> anyhow::Result<Self> {
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(root)?;
        let batch = format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        );
        let mut pending = Self { files: Vec::new() };
        for (index, (output, entry)) in payload
            .as_object_mut()
            .context("Plasma assignments must be an object")?
            .iter_mut()
            .enumerate()
        {
            let id = format!("{batch}-{index}");
            let path = root.join(format!("{id}.json"));
            let mut file =
                std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path)?;
            pending.files.push((output.clone(), path));
            file.write_all(br#"{"state":"pending"}"#)?;
            entry["presentationId"] = serde_json::Value::String(id);
        }
        anyhow::ensure!(
            !pending.files.is_empty(),
            "Plasma has no connected output to present the wallpaper"
        );
        Ok(pending)
    }

    pub(super) fn wait(&self, timeout: Duration) -> anyhow::Result<()> {
        let started = Instant::now();
        loop {
            let mut waiting = Vec::new();
            for (output, path) in &self.files {
                let mut bytes = Vec::new();
                let status = std::fs::File::open(path)
                    .and_then(|file| file.take(16_384).read_to_end(&mut bytes))
                    .ok()
                    .and_then(|_| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
                match status.as_ref().and_then(|status| status["state"].as_str()) {
                    Some("ready") => {}
                    Some("error") => anyhow::bail!(
                        "Plasma could not display the wallpaper on {output}: {}",
                        status
                            .as_ref()
                            .and_then(|status| status["error"].as_str())
                            .unwrap_or("renderer failed")
                    ),
                    _ => waiting.push(output.as_str()),
                }
            }
            if waiting.is_empty() {
                return Ok(());
            }
            anyhow::ensure!(
                started.elapsed() < timeout,
                "Plasma did not confirm a wallpaper frame on {}. Check the Plasma logs and install matching Deck, Paper and skwd-paper-plasma versions.",
                waiting.join(", ")
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        for (_, path) in &self.files {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests;
