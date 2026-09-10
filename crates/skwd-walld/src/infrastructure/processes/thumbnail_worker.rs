use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::{Arc, Mutex, Weak, mpsc};
use std::time::{Duration, Instant};

use anyhow::Context;
use serde::{Serialize, de::DeserializeOwned};
use skwd_wall_core::lock;

pub(super) type Children = Mutex<Vec<Weak<Mutex<Child>>>>;

pub(super) struct Worker {
    child: Arc<Mutex<Child>>,
    input: Option<ChildStdin>,
    replies: Option<mpsc::Receiver<std::io::Result<Vec<u8>>>>,
    reader: Option<std::thread::JoinHandle<()>>,
}

impl Worker {
    pub(super) fn spawn(
        mut command: std::process::Command,
        children: &Children,
    ) -> anyhow::Result<Self> {
        let mut child = command.stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;
        let input = child.stdin.take().context("thumbnail worker stdin unavailable")?;
        let output = child.stdout.take().context("thumbnail worker stdout unavailable")?;
        let child = Arc::new(Mutex::new(child));
        let mut children = lock(children);
        children.retain(|child| child.strong_count() > 0);
        children.push(Arc::downgrade(&child));
        drop(children);
        let (sender, replies) = mpsc::sync_channel(1);
        let reader = std::thread::spawn(move || {
            let mut output = BufReader::new(output);
            loop {
                let mut line = Vec::new();
                let result = (&mut output)
                    .take(1024 * 1024 + 1)
                    .read_until(b'\n', &mut line)
                    .and_then(|size| {
                        if size == 0 {
                            return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
                        }
                        if size > 1024 * 1024 {
                            return Err(std::io::Error::other("thumbnail worker reply too large"));
                        }
                        Ok(line)
                    });
                let failed = result.is_err();
                if sender.send(result).is_err() || failed {
                    break;
                }
            }
        });
        Ok(Self { child, input: Some(input), replies: Some(replies), reader: Some(reader) })
    }

    pub(super) fn exchange<R: DeserializeOwned>(
        &mut self,
        request: &impl Serialize,
    ) -> anyhow::Result<R> {
        let input = self.input.as_mut().context("thumbnail worker stdin closed")?;
        serde_json::to_writer(&mut *input, request)?;
        input.write_all(b"\n")?;
        input.flush()?;
        let line = self
            .replies
            .as_ref()
            .context("thumbnail worker stopped")?
            .recv_timeout(Duration::from_secs(30))
            .context("thumbnail worker timed out or exited")??;
        Ok(serde_json::from_slice(&line)?)
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.input.take();
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let mut child = lock(&self.child);
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => {}
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break;
                }
            }
            drop(child);
            std::thread::sleep(Duration::from_millis(5));
        }
        self.replies.take();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

#[cfg(test)]
mod tests;
