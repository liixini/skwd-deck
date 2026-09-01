use super::{HistoryEntry, OutputHistory};

fn entry(path: &str) -> HistoryEntry {
    HistoryEntry::new("static", path, "", true, 0)
}

#[test]
fn record_dedup() {
    let mut history = OutputHistory::default();
    history.record(&entry("/a"), None, 50);
    history.record(&entry("/a"), None, 50);

    assert_eq!(history.entries, vec![entry("/a")]);
    assert_eq!(history.pos, 0);
}

#[test]
fn record_after_back_truncates() {
    let mut history = OutputHistory::default();
    for path in ["/a", "/b", "/c"] {
        history.record(&entry(path), None, 50);
    }
    assert_eq!(history.back(), Some(entry("/b")));

    history.record(&entry("/d"), None, 50);

    assert_eq!(history.entries, vec![entry("/a"), entry("/b"), entry("/d")]);
    assert_eq!(history.forward(), None);
}

#[test]
fn cap_preserves_cursor() {
    let mut history = OutputHistory::default();
    for index in 0..10 {
        history.record(&entry(&format!("/w{index}")), None, 3);
    }

    assert_eq!(history.entries, vec![entry("/w7"), entry("/w8"), entry("/w9")]);
    assert_eq!(history.pos, 2);
}

#[test]
fn prior_seeds_once() {
    let mut history = OutputHistory::default();
    history.record(&entry("/new"), Some(&entry("/old")), 50);
    history.record(&entry("/next"), Some(&entry("/ignored")), 50);

    assert_eq!(history.entries, vec![entry("/old"), entry("/new"), entry("/next")]);
}

#[test]
fn invalid_prior_not_seeded() {
    let mut history = OutputHistory::default();
    history.record(&entry("/new"), Some(&HistoryEntry::new("static", "", "", true, 0)), 50);

    assert_eq!(history.entries, vec![entry("/new")]);
}
