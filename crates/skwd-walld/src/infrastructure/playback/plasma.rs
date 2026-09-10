use std::collections::HashMap;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::Context;
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;

use super::fullscreen::Snapshot;

static POLICY: OnceLock<watch::Sender<HashMap<String, bool>>> = OnceLock::new();

fn policy() -> &'static watch::Sender<HashMap<String, bool>> {
    POLICY.get_or_init(|| watch::channel(HashMap::new()).0)
}

pub(super) fn refresh_policy(
    state: &skwd_wall_core::WallState,
    outputs: &std::collections::HashSet<String>,
) {
    let next = outputs
        .iter()
        .map(|output| (output.clone(), state.renderers().paused_for(output)))
        .collect();
    policy().send_if_modified(|previous| {
        if *previous == next {
            false
        } else {
            *previous = next;
            true
        }
    });
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Observation {
    version: u32,
    output: String,
    supported: bool,
    fullscreen: bool,
    maximized: bool,
}

impl Observation {
    fn decode(bytes: &[u8]) -> anyhow::Result<Self> {
        let observation: Self = serde_json::from_slice(bytes)?;
        anyhow::ensure!(observation.version == 1, "unknown Plasma window-state version");
        anyhow::ensure!(
            !observation.output.is_empty()
                && observation.output.len() <= 256
                && !observation.output.chars().any(char::is_control),
            "invalid Plasma output name"
        );
        Ok(observation)
    }
}

struct SocketFile {
    path: PathBuf,
    identity: (u64, u64),
}

impl Drop for SocketFile {
    fn drop(&mut self) {
        if let Ok(metadata) = std::fs::symlink_metadata(&self.path)
            && (metadata.dev(), metadata.ino()) == self.identity
        {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

async fn bind(path: &Path) -> anyhow::Result<(UnixListener, SocketFile)> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        anyhow::ensure!(metadata.file_type().is_socket(), "window-state endpoint is not a socket");
        anyhow::ensure!(
            metadata.uid() == unsafe { libc::geteuid() },
            "window-state endpoint belongs to another user"
        );
        anyhow::ensure!(
            UnixStream::connect(path).await.is_err(),
            "window-state endpoint is already serving"
        );
        std::fs::remove_file(path)?;
    }
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    let metadata = std::fs::symlink_metadata(path)?;
    Ok((listener, SocketFile { path: path.to_owned(), identity: (metadata.dev(), metadata.ino()) }))
}

fn snapshot(observations: &HashMap<u64, Observation>) -> Snapshot {
    let supported = observations.values().any(|value| value.supported);
    Snapshot {
        supported,
        maximized_supported: supported,
        backend: if supported { "plasma" } else { "unavailable" },
        outputs: observations
            .values()
            .filter(|value| value.supported && value.fullscreen)
            .map(|value| value.output.clone())
            .collect(),
        maximized_outputs: observations
            .values()
            .filter(|value| value.supported && value.maximized)
            .map(|value| value.output.clone())
            .collect(),
        observed_outputs: observations.values().map(|value| value.output.clone()).collect(),
    }
}

async fn receive(
    stream: UnixStream,
    id: u64,
    events: &mpsc::Sender<(u64, Option<Observation>)>,
    mut policies: watch::Receiver<HashMap<String, bool>>,
) -> anyhow::Result<()> {
    let (read, mut write) = stream.into_split();
    let mut reader = BufReader::new(read);
    let mut output = String::new();
    let mut previous = None;
    let mut bytes = Vec::new();
    loop {
        let mut limited = (&mut reader).take((4097 - bytes.len()) as u64);
        tokio::select! {
            count = limited.read_until(b'\n', &mut bytes) => {
                if count? == 0 { return Ok(()); }
                anyhow::ensure!(bytes.len() <= 4096 && bytes.last() == Some(&b'\n'), "invalid Plasma observation frame");
                let observation = Observation::decode(&bytes)?;
                output.clone_from(&observation.output);
                events.send((id, Some(observation))).await?;
                bytes.clear();
            }
            changed = policies.changed() => { if changed.is_err() { return Ok(()); } }
        }
        let paused = policies.borrow().get(&output).copied();
        if paused != previous
            && let Some(paused) = paused
        {
            let line = format!("{{\"version\":1,\"paused\":{paused}}}\n");
            tokio::time::timeout(
                std::time::Duration::from_secs(2),
                write.write_all(line.as_bytes()),
            )
            .await??;
            previous = Some(paused);
        }
    }
}

async fn serve(
    path: &Path,
    sender: watch::Sender<Snapshot>,
    policies: watch::Receiver<HashMap<String, bool>>,
) -> anyhow::Result<()> {
    let (listener, _file) = bind(path).await.context("bind Plasma window-state socket")?;
    let (events, mut receive_events) = mpsc::channel(64);
    let mut tasks = JoinSet::new();
    let mut observations = HashMap::new();
    let mut next_id = 0_u64;
    loop {
        tokio::select! {
            accepted = listener.accept(), if tasks.len() < 32 => {
                let (stream, _) = accepted?;
                if stream.peer_cred()?.uid() != unsafe { libc::geteuid() } { continue; }
                next_id += 1;
                let id = next_id;
                let events = events.clone();
                let policies = policies.clone();
                tasks.spawn(async move {
                    let result = receive(stream, id, &events, policies).await;
                    let _ = events.send((id, None)).await;
                    result
                });
            }
            Some((id, observation)) = receive_events.recv() => {
                if let Some(observation) = observation {
                    observations.insert(id, observation);
                } else { observations.remove(&id); }
                sender.send_replace(snapshot(&observations));
            }
            Some(result) = tasks.join_next(), if !tasks.is_empty() => {
                if let Err(error) = result? { log::debug!("Plasma window-state stream: {error:#}"); }
            }
            () = sender.closed() => return Ok(()),
        }
    }
}

pub(super) async fn run(sender: watch::Sender<Snapshot>) {
    let path = wall_proto::resolve_socket().with_file_name("window-state.sock");
    if let Err(error) = serve(&path, sender.clone(), policy().subscribe()).await {
        log::warn!("Plasma window-state detection: {error:#}");
    }
    sender.send_replace(Snapshot::default());
}

#[cfg(test)]
mod tests;
