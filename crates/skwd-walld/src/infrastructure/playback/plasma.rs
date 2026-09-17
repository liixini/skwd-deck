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
static PUBLISHED: OnceLock<watch::Sender<u64>> = OnceLock::new();

fn policy() -> &'static watch::Sender<HashMap<String, bool>> {
    POLICY.get_or_init(|| watch::channel(HashMap::new()).0)
}

fn published() -> &'static watch::Sender<u64> {
    PUBLISHED.get_or_init(|| watch::channel(0).0)
}

fn assignments_published() {
    published().send_modify(|generation| *generation = generation.wrapping_add(1));
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

fn valid_output(output: &str) -> bool {
    !output.is_empty() && output.len() <= 256 && !output.chars().any(char::is_control)
}

impl Observation {
    fn from_value(value: serde_json::Value) -> anyhow::Result<Self> {
        let observation: Self = serde_json::from_value(value)?;
        anyhow::ensure!(observation.version == 1, "unknown Plasma window-state version");
        anyhow::ensure!(valid_output(&observation.output), "invalid Plasma output name");
        Ok(observation)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Subscription {
    version: u32,
    output: String,
    subscribe: String,
}

enum Frame {
    Observation(Observation),
    Subscribe(String),
}

impl Frame {
    fn decode(bytes: &[u8]) -> anyhow::Result<Self> {
        let value: serde_json::Value = serde_json::from_slice(bytes)?;
        if value.get("version").and_then(serde_json::Value::as_u64) != Some(2) {
            return Ok(Self::Observation(Observation::from_value(value)?));
        }
        let request: Subscription = serde_json::from_value(value)?;
        anyhow::ensure!(
            request.version == 2 && request.subscribe == "assignments",
            "unknown Plasma subscription"
        );
        anyhow::ensure!(valid_output(&request.output), "invalid Plasma output name");
        Ok(Self::Subscribe(request.output))
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
    mut published: watch::Receiver<u64>,
) -> anyhow::Result<()> {
    let (read, mut write) = stream.into_split();
    let mut reader = BufReader::new(read);
    let mut output = String::new();
    let mut previous = None;
    let mut subscription = None;
    let mut delivered = None;
    let mut bytes = Vec::new();
    loop {
        let mut limited = (&mut reader).take((4097 - bytes.len()) as u64);
        tokio::select! {
            count = limited.read_until(b'\n', &mut bytes) => {
                if count? == 0 { return Ok(()); }
                anyhow::ensure!(bytes.len() <= 4096 && bytes.last() == Some(&b'\n'), "invalid Plasma observation frame");
                match Frame::decode(&bytes)? {
                    Frame::Observation(observation) => {
                        output.clone_from(&observation.output);
                        events.send((id, Some(observation))).await?;
                    }
                    Frame::Subscribe(requested) => {
                        anyhow::ensure!(output == requested, "Plasma subscription names another output");
                        subscription = Some(skwd_wall_core::plasma::channel::subscribe(&requested));
                    }
                }
                bytes.clear();
            }
            changed = policies.changed() => { if changed.is_err() { return Ok(()); } }
            changed = published.changed() => { if changed.is_err() { return Ok(()); } }
        }
        let Some(paused) = policies.borrow().get(&output).copied() else { continue };
        let entry = subscription
            .as_ref()
            .and_then(|_| skwd_wall_core::plasma::channel::entry(&output))
            .filter(|entry| delivered.as_ref() != Some(entry));
        if previous == Some(paused) && entry.is_none() {
            continue;
        }
        let mut line =
            serde_json::json!({"version": 1, "paused": paused, "capabilities": ["assignments"]});
        if let Some(entry) = &entry {
            line["entry"] = entry.clone();
        }
        let mut text = line.to_string();
        text.push('\n');
        tokio::time::timeout(std::time::Duration::from_secs(2), write.write_all(text.as_bytes()))
            .await??;
        previous = Some(paused);
        if entry.is_some() {
            delivered = entry;
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
                let published = published().subscribe();
                tasks.spawn(async move {
                    let result = receive(stream, id, &events, policies, published).await;
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
    skwd_wall_core::plasma::channel::set_notifier(assignments_published);
    let path = wall_proto::resolve_socket().with_file_name("window-state.sock");
    if let Err(error) = serve(&path, sender.clone(), policy().subscribe()).await {
        log::warn!("Plasma window-state detection: {error:#}");
    }
    sender.send_replace(Snapshot::default());
}

#[cfg(test)]
mod tests;
