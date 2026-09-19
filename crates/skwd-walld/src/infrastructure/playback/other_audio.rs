use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::watch;
use tokio::time::Instant;

use skwd_wall_core::WallState;
use skwd_wall_core::backend::renderers::RendererSupervision;

const RELEASE_GRACE: Duration = Duration::from_millis(1500);
const RETRY_DELAY: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub supported: bool,
    pub playing: bool,
}

pub(crate) fn is_stream_event(line: &str) -> bool {
    line.contains("sink-input")
}

pub(crate) fn other_stream_playing(inputs: &Value, own_pids: &HashSet<u32>) -> bool {
    inputs.as_array().is_some_and(|inputs| {
        inputs.iter().any(|input| {
            let corked = input.get("corked").and_then(Value::as_bool).unwrap_or(true);
            let muted = input.get("mute").and_then(Value::as_bool).unwrap_or(false);
            let properties = input.get("properties");
            let property = |key: &str| properties.and_then(|props| props.get(key)?.as_str());
            let own_binary = property("application.process.binary") == Some("skwd-wall-vk");
            let own_name = matches!(
                property("application.name"),
                Some(
                    "skwd-wall-vk" | "PipeWire ALSA [skwd-wall-vk]" | "ALSA plug-in [skwd-wall-vk]"
                )
            ) || property("node.name") == Some("alsa_playback.skwd-wall-vk");
            let own_pid = property("application.process.id")
                .and_then(|pid| pid.parse::<u32>().ok())
                .is_some_and(|pid| own_pids.contains(&pid));
            !corked && !muted && !own_binary && !own_name && !own_pid
        })
    })
}

pub(super) async fn run(
    mut enabled: watch::Receiver<bool>,
    sender: watch::Sender<Snapshot>,
    state: Arc<WallState>,
) {
    loop {
        if *enabled.borrow() {
            tokio::select! {
                result = subscribed(&sender, &state) => {
                    match result {
                        Ok(()) => log::warn!("other-audio detection: pactl subscribe ended"),
                        Err(error) => log::warn!("other-audio detection unavailable: {error}"),
                    }
                }
                changed = enabled.changed() => { if changed.is_err() { return; } }
            }
            sender.send_replace(Snapshot::default());
            if *enabled.borrow() {
                tokio::select! {
                    () = tokio::time::sleep(RETRY_DELAY) => {}
                    changed = enabled.changed() => { if changed.is_err() { return; } }
                }
            }
        } else if enabled.changed().await.is_err() {
            return;
        }
    }
}

async fn subscribed(sender: &watch::Sender<Snapshot>, state: &WallState) -> std::io::Result<()> {
    let mut command = crate::infrastructure::proc::tool_async("pactl");
    command
        .arg("subscribe")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let mut child = crate::infrastructure::proc::spawn(&mut command)?;
    let stdout = child.stdout.take().ok_or_else(|| std::io::Error::other("pactl stdout"))?;
    let mut lines = BufReader::new(stdout).lines();
    let mut playing = observe(state).await?;
    publish(sender, playing);
    let mut release_at: Option<Instant> = None;
    loop {
        tokio::select! {
            line = lines.next_line() => {
                let Some(line) = line? else { break };
                if !is_stream_event(&line) {
                    continue;
                }
                if observe(state).await? {
                    release_at = None;
                    if !playing {
                        playing = true;
                        publish(sender, playing);
                    }
                } else if playing && release_at.is_none() {
                    release_at = Some(Instant::now() + RELEASE_GRACE);
                }
            }
            () = async { match release_at { Some(deadline) => tokio::time::sleep_until(deadline).await, None => std::future::pending().await } } => {
                release_at = None;
                playing = false;
                publish(sender, playing);
            }
        }
    }
    let _ = child.kill().await;
    Ok(())
}

fn publish(sender: &watch::Sender<Snapshot>, playing: bool) {
    sender.send_if_modified(|previous| {
        let next = Snapshot { supported: true, playing };
        let changed = *previous != next;
        *previous = next;
        changed
    });
}

async fn observe(state: &WallState) -> std::io::Result<bool> {
    let mut command = crate::infrastructure::proc::tool_async("pactl");
    command
        .args(["-f", "json", "list", "sink-inputs"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let output = command.output().await?;
    if !output.status.success() {
        return Err(std::io::Error::other("pactl list sink-inputs failed"));
    }
    let inputs: Value = serde_json::from_slice(&output.stdout).map_err(std::io::Error::other)?;
    let renderers = state.renderers();
    let own_pids: HashSet<u32> =
        renderers.wallpaper_pids().into_iter().chain(renderers.scene_pids()).collect();
    Ok(other_stream_playing(&inputs, &own_pids))
}

#[cfg(test)]
mod tests;
