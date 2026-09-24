use super::*;

#[derive(Default)]
struct Events(Mutex<Vec<serde_json::Value>>);

impl EventPublisher for Events {
    fn publish(&self, _: &str, data: serde_json::Value) {
        self.0.lock().unwrap().push(data);
    }
}

fn write(root: &Path, id: &str, document: &serde_json::Value) {
    std::fs::create_dir_all(root.join(id)).unwrap();
    std::fs::write(root.join(id).join("project.json"), document.to_string()).unwrap();
}

#[test]
fn completes_only_after_dependency_download_and_reuses_shared_parent() {
    let root = tempfile::tempdir().unwrap();
    let events = Events::default();
    let mut fetched = Vec::new();
    assert!(download(&events, root.path(), &["2".into(), "3".into()], |progress, ids| {
        let id = &ids[0];
        fetched.push(id.clone());
        if id == "1" {
            write(root.path(), id, &json!({"type":"scene"}));
            std::fs::write(root.path().join("1/scene.pkg"), b"fixture").unwrap();
        } else {
            write(root.path(), id, &json!({"dependency":"1","preset":{}}));
        }
        assert!(
            !events
                .0
                .lock()
                .unwrap()
                .iter()
                .any(|event| event["id"] == "2" && event["status"] == "done")
                || id == "3"
        );
        progress.publish(ev::DOWNLOAD, json!({"id":id,"status":"done","progress":1.0}));
        true
    }));
    assert_eq!(fetched, ["2", "1", "3"]);
    let recorded = events.0.lock().unwrap();
    let done: Vec<_> = recorded
        .iter()
        .filter(|event| event["status"] == "done")
        .map(|event| event["id"].as_str().unwrap())
        .collect();
    assert_eq!(done, ["2", "3"]);
    assert!(recorded.iter().all(|event| event["id"] != "1"));
}

#[test]
fn repairs_an_existing_preset_and_reports_parent_failure() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "2", &json!({"dependency":"1","preset":{}}));
    let events = Events::default();
    assert!(!download(&events, root.path(), &["2".into()], |progress, ids| {
        assert_eq!(ids, ["1"]);
        progress.publish(
            ev::DOWNLOAD,
            json!({"id":"1","status":"error","message":"parent unavailable"}),
        );
        false
    }));
    let recorded = events.0.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0]["id"], "2");
    assert_eq!(recorded[0]["status"], "error");
    assert_eq!(recorded[0]["message"], "parent unavailable");
}

#[test]
fn rejects_cycles_and_unsafe_ids_without_fetching() {
    for documents in [
        vec![
            ("1", json!({"dependency":"2","preset":{}})),
            ("2", json!({"dependency":"1","preset":{}})),
        ],
        vec![("1", json!({"dependency":"../escape","preset":{}}))],
    ] {
        let root = tempfile::tempdir().unwrap();
        for (id, document) in documents {
            write(root.path(), id, &document);
        }
        let events = Events::default();
        assert!(!download(&events, root.path(), &["1".into()], |_, _| panic!("unexpected fetch")));
        assert_eq!(events.0.lock().unwrap().last().unwrap()["status"], "error");
    }
}

#[test]
fn retries_a_parent_whose_package_is_missing() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "2", &json!({"dependency":"1","preset":{}}));
    write(root.path(), "1", &json!({"type":"scene"}));
    assert!(download(&Events::default(), root.path(), &["2".into()], |_, ids| {
        assert_eq!(ids, ["1"]);
        std::fs::write(root.path().join("1/scene.pkg"), b"repaired").unwrap();
        true
    }));
}

#[test]
fn links_an_already_installed_parent_next_to_a_steam_item() {
    let root = tempfile::tempdir().unwrap();
    let library = root.path().join("library");
    let steam = root.path().join("steam");
    write(&steam, "2", &json!({"dependency":"1","preset":{}}));
    write(&steam, "1", &json!({"type":"scene"}));
    std::fs::write(steam.join("1/scene.pkg"), b"fixture").unwrap();
    super::super::reconcile_we_item(&library, "2", &steam.join("2"));
    assert!(download(&Events::default(), &library, &["2".into()], |_, _| panic!(
        "parent already installed"
    )));
    assert_eq!(library.join("1").canonicalize().unwrap(), steam.join("1"));
}
