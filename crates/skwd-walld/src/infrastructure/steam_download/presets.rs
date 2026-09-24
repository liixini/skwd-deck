use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;

use paper_control::we_project::{MAX_PRESET_DEPTH, Project, dependency, read};
use serde_json::json;
use wall_proto::{dl_status, ev};

use crate::backend::events::EventPublisher;

struct Progress<'a> {
    publisher: &'a dyn EventPublisher,
    requested: &'a str,
    error: Mutex<Option<(String, String)>>,
}

impl EventPublisher for Progress<'_> {
    fn publish(&self, event: &str, mut data: serde_json::Value) {
        if event != ev::DOWNLOAD {
            self.publisher.publish(event, data);
            return;
        }
        if matches!(data["status"].as_str(), Some(dl_status::ERROR | dl_status::AUTH_ERROR)) {
            *skwd_wall_core::lock(&self.error) = Some((
                data["status"].as_str().unwrap().to_string(),
                data["message"].as_str().unwrap_or("Steam download failed").to_string(),
            ));
            return;
        }
        let downloaded = data["id"].as_str().unwrap_or_default().to_string();
        data["id"] = json!(self.requested);
        if data["status"] == dl_status::DONE {
            data["status"] = json!(dl_status::DOWNLOADING);
        }
        if downloaded != self.requested {
            data["message"] = json!(format!("Downloading required Workshop item {downloaded}"));
        }
        self.publisher.publish(event, data);
    }
}

pub(super) fn download(
    publisher: &dyn EventPublisher,
    directory: &Path,
    ids: &[String],
    mut fetch: impl FnMut(&dyn EventPublisher, &[String]) -> bool,
) -> bool {
    let mut completed = false;
    for id in ids {
        let progress = Progress { publisher, requested: id, error: Mutex::new(None) };
        match install(directory, id, &progress, &mut fetch) {
            Ok(()) => {
                super::steam_dl_event(publisher, id, dl_status::DONE, 1.0);
                completed = true;
            }
            Err(error) => {
                let recorded = skwd_wall_core::lock(&progress.error);
                let status =
                    recorded.as_ref().map_or(dl_status::ERROR, |(status, _)| status.as_str());
                super::steam_dl_msg(publisher, id, status, 0.0, &error.to_string());
            }
        }
    }
    completed
}

fn install(
    directory: &Path,
    requested: &str,
    progress: &Progress<'_>,
    fetch: &mut impl FnMut(&dyn EventPublisher, &[String]) -> bool,
) -> anyhow::Result<()> {
    let mut id = requested.to_string();
    let mut visited = HashSet::new();
    loop {
        anyhow::ensure!(
            visited.insert(id.clone()),
            "Wallpaper Engine preset dependency cycle at {id}"
        );
        anyhow::ensure!(
            visited.len() <= MAX_PRESET_DEPTH,
            "Wallpaper Engine preset dependency chain is too deep"
        );
        let item = directory.join(&id);
        let existing = read(&item).ok();
        let incomplete = existing.as_ref().is_some_and(|document| {
            matches!(
                document.get("type").and_then(serde_json::Value::as_str),
                Some("scene" | "video")
            ) && validate(&item).is_err()
        });
        if (existing.is_none() || incomplete) && !fetch(progress, &[id.clone()]) {
            let error = skwd_wall_core::lock(&progress.error).as_ref().map_or_else(
                || format!("Download failed for Workshop item {id}"),
                |(_, message)| message.clone(),
            );
            anyhow::bail!("{error}");
        }
        let document = read(&item)?;
        let Some(parent) = dependency(&document)? else { break };
        if let Ok(actual) = item.canonicalize() {
            let sibling = actual.parent().unwrap().join(&parent);
            if sibling.join("project.json").is_file() {
                super::reconcile_we_item(directory, &parent, &sibling);
            }
        }
        id = parent;
    }
    validate(&directory.join(requested))
}

pub(crate) fn validate(directory: &Path) -> anyhow::Result<()> {
    let project = Project::resolve(directory)?;
    match project
        .document
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "scene" => {
            project.scene_package()?;
        }
        "video" => {
            let file =
                project.document.get("file").and_then(serde_json::Value::as_str).unwrap_or("");
            anyhow::ensure!(
                skwd_wall_core::we::safe_item_join(&project.source, file)
                    .is_some_and(|path| path.is_file()),
                "Wallpaper Engine video file is missing or unsafe"
            );
        }
        kind => anyhow::bail!("unsupported Wallpaper Engine project type {kind:?}"),
    }
    Ok(())
}

#[cfg(test)]
#[path = "presets_tests.rs"]
mod tests;
