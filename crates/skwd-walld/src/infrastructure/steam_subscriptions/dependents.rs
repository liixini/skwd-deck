use std::path::{Path, PathBuf};

use paper_control::we_project::{Project, read};
use skwd_wall_core::we::SteamTracking;

use crate::infrastructure::steam_download::SteamInventory;

const PROJECT_TITLE: &str = "title";

pub(super) struct Candidate {
    pub id: String,
    path: PathBuf,
    actual: PathBuf,
    sibling: bool,
}

pub(super) fn candidates(source: &Path, target: &Path) -> Vec<Candidate> {
    let mut found = std::fs::read_dir(source)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if !path.join(super::PROJECT_FILE).is_file() {
                return None;
            }
            Some(Candidate {
                id: entry.file_name().to_string_lossy().into_owned(),
                actual: path.canonicalize().ok()?,
                path,
                sibling: false,
            })
        })
        .collect::<Vec<_>>();
    if let Some(sibling) = target.parent().filter(|sibling| *sibling != source) {
        found.extend(super::item_paths(sibling).into_iter().map(|(id, actual)| Candidate {
            path: sibling.join(&id),
            id,
            actual,
            sibling: true,
        }));
    }
    found
}

pub(super) fn blocking(
    candidates: &[Candidate],
    target: &Path,
    inventory: &SteamInventory,
    tracking: &SteamTracking,
) -> Vec<String> {
    let mut names = Vec::new();
    for candidate in candidates {
        if candidate.actual == target {
            continue;
        }
        let native = candidate.sibling
            || tracking.hidden.get(&candidate.id) == Some(&candidate.actual)
            || inventory
                .installed
                .get(&candidate.id)
                .is_some_and(|folder| super::matches_path(folder, &candidate.actual))
            || tracking.owned.get(&inventory.account).and_then(|owned| owned.get(&candidate.id))
                == Some(&candidate.actual);
        if native && !inventory.subscribed.contains(&candidate.id) {
            continue;
        }
        let Ok(project) = Project::resolve(&candidate.path) else { continue };
        if !project.directories.iter().skip(1).any(|directory| directory == target) {
            continue;
        }
        let title = read(&candidate.path)
            .ok()
            .and_then(|document| document[PROJECT_TITLE].as_str().map(str::to_owned))
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| candidate.id.clone());
        names.push(format!("{title} ({})", candidate.id));
    }
    names.sort();
    names.dedup();
    names
}

#[cfg(test)]
#[path = "dependents/tests.rs"]
mod tests;
