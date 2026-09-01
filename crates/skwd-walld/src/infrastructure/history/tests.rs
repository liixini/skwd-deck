use crate::backend::history::HistoryRepository;
use crate::domain::history::HistoryEntry;

use super::FileHistoryRepository;

fn entry(path: &str) -> HistoryEntry {
    HistoryEntry::new("static", path, "", true, 0)
}

#[test]
fn fan_out_and_navigate() {
    let directory = tempfile::tempdir().unwrap();
    let repository = FileHistoryRepository::new(directory.path());
    let live = vec!["DP-1".to_string(), "DP-2".to_string()];

    repository.record("*", &entry("/a"), None, 50, &live);
    repository.record("*", &entry("/b"), None, 50, &live);

    let moved = repository.navigate("*", false, &live);
    assert_eq!(moved, vec![("DP-1".to_string(), entry("/a")), ("DP-2".to_string(), entry("/a"))]);
    let listed = repository.list("*");
    assert_eq!(
        listed.iter().find(|(output, _)| output == "DP-1").map(|(_, history)| history.pos),
        Some(0),
    );
}

#[test]
fn malformed_storage_recovers() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("history.json"), b"{ invalid json ]").unwrap();
    let repository = FileHistoryRepository::new(directory.path());

    repository.record("DP-1", &entry("/a"), None, 50, &[]);

    assert_eq!(repository.list("DP-1")[0].1.entries[0].path, "/a");
}
