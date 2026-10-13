use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use skwd_wall_core::we::SteamTracking;

use super::{blocking, candidates};
use crate::infrastructure::steam_download::SteamInventory;

fn item(root: &Path, id: &str, project: &Value) -> PathBuf {
    let path = root.join(id);
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("project.json"), project.to_string()).unwrap();
    path
}

fn inventory(subscribed: &[&str]) -> SteamInventory {
    SteamInventory {
        account: "account".into(),
        subscribed: subscribed.iter().map(|id| (*id).to_string()).collect(),
        installed: HashMap::new(),
    }
}

#[test]
fn dependent_chains() {
    let root = tempfile::tempdir().unwrap();
    let parent = item(root.path(), "10", &json!({"title":"Parent","type":"scene"}));
    item(root.path(), "20", &json!({"title":"Direct","dependency":"10","preset":{}}));
    item(root.path(), "30", &json!({"title":"Chain","dependency":"20","preset":{}}));
    let found = candidates(root.path(), &parent);
    assert_eq!(
        blocking(&found, &parent, &inventory(&["10", "20", "30"]), &SteamTracking::default()),
        ["Chain (30)", "Direct (20)"]
    );
}

#[test]
fn sibling_preset() {
    let root = tempfile::tempdir().unwrap();
    let native = root.path().join("native");
    let curated = root.path().join("curated");
    std::fs::create_dir_all(&curated).unwrap();
    let parent = item(&native, "10", &json!({"title":"Parent","type":"scene"}));
    item(&native, "20", &json!({"title":"Sibling","dependency":"10","preset":{}}));
    std::os::unix::fs::symlink(&parent, curated.join("10")).unwrap();
    let found = candidates(&curated, &parent);
    assert_eq!(
        blocking(&found, &parent, &inventory(&["10", "20"]), &SteamTracking::default()),
        ["Sibling (20)"]
    );
}

#[test]
fn hidden_child() {
    let root = tempfile::tempdir().unwrap();
    let native = root.path().join("native");
    let curated = root.path().join("curated");
    std::fs::create_dir_all(&curated).unwrap();
    let parent = item(&native, "10", &json!({"title":"Parent","type":"scene"}));
    let child = item(&native, "20", &json!({"title":"Child","dependency":"10","preset":{}}));
    std::os::unix::fs::symlink(&parent, curated.join("10")).unwrap();
    std::os::unix::fs::symlink(&child, curated.join("20")).unwrap();
    let mut snapshot = inventory(&["10"]);
    snapshot.installed.insert("20".into(), child.clone());
    let tracking =
        SteamTracking { hidden: HashMap::from([("20".into(), child)]), ..SteamTracking::default() };
    let found = candidates(&curated, &parent);
    assert!(blocking(&found, &parent, &snapshot, &tracking).is_empty());
    snapshot.subscribed.insert("20".into());
    assert_eq!(blocking(&found, &parent, &snapshot, &tracking), ["Child (20)"]);
}

#[test]
fn same_id_other_path() {
    let root = tempfile::tempdir().unwrap();
    let native = root.path().join("native");
    let manual = root.path().join("manual");
    let target = item(&native, "10", &json!({"title":"Native","type":"scene"}));
    item(&manual, "10", &json!({"title":"Manual","type":"scene"}));
    item(&manual, "20", &json!({"title":"Manual child","dependency":"10","preset":{}}));
    let found = candidates(&manual, &target);
    assert!(blocking(&found, &target, &inventory(&["10"]), &SteamTracking::default()).is_empty());
}

#[test]
fn named_manual_preset() {
    let root = tempfile::tempdir().unwrap();
    let native = root.path().join("native");
    let curated = root.path().join("curated");
    std::fs::create_dir_all(&curated).unwrap();
    let parent = item(&native, "10", &json!({"title":"Parent","type":"scene"}));
    std::os::unix::fs::symlink(&parent, curated.join("10")).unwrap();
    item(
        &curated,
        "manual-preset",
        &json!({"title":"Manual preset","dependency":"10","preset":{}}),
    );
    let found = candidates(&curated, &parent);
    assert_eq!(
        blocking(&found, &parent, &inventory(&["10"]), &SteamTracking::default()),
        ["Manual preset (manual-preset)"]
    );
}
