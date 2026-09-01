use std::collections::BTreeSet;

use super::{PollAdvance, PollDelta, PollingRoot};
use crate::testenv::tmp;

fn complete(root: &mut PollingRoot, budget: usize) -> PollDelta {
    for _ in 0..64 {
        match root.advance(budget) {
            PollAdvance::Pending => {}
            PollAdvance::Complete(delta) => return delta,
            PollAdvance::Failed(error) => panic!("poll failed: {error}"),
        }
    }
    panic!("poll did not complete")
}

#[test]
fn bounded_sweeps_converge_without_duplicates() {
    let dir = tmp("poll-bounded");
    for name in ["a.png", "b.png", "c.png"] {
        std::fs::write(dir.join(name), name).unwrap();
    }
    let mut root = PollingRoot::new(dir.clone(), String::from("injected watch failure"));
    assert!(matches!(root.advance(1), PollAdvance::Pending));
    let first = complete(&mut root, 1);
    assert!(first.initial);
    assert_eq!(first.changed.len(), 3);
    assert!(first.removed.is_empty());
    let unchanged = complete(&mut root, 1);
    assert!(!unchanged.initial);
    assert_eq!(unchanged, PollDelta::default());
    assert!(root.last_completed_sweep_unix_ms.is_some());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn detects_add_change_move_and_delete_once() {
    let dir = tmp("poll-delta");
    let changed = dir.join("changed.png");
    let moved_from = dir.join("move-from.png");
    let deleted = dir.join("deleted.png");
    for path in [&changed, &moved_from, &deleted] {
        std::fs::write(path, b"old").unwrap();
    }
    let mut root = PollingRoot::new(dir.clone(), String::from("injected watch failure"));
    let _ = complete(&mut root, 32);

    std::fs::write(&changed, b"new-and-longer").unwrap();
    let added = dir.join("added.png");
    std::fs::write(&added, b"new").unwrap();
    let moved_to = dir.join("move-to.png");
    std::fs::rename(&moved_from, &moved_to).unwrap();
    std::fs::remove_file(&deleted).unwrap();

    let delta = complete(&mut root, 32);
    assert_eq!(
        delta.changed.into_iter().collect::<BTreeSet<_>>(),
        BTreeSet::from([added, changed, moved_to])
    );
    assert_eq!(
        delta.removed.into_iter().collect::<BTreeSet<_>>(),
        BTreeSet::from([deleted, moved_from])
    );
    assert_eq!(complete(&mut root, 32), PollDelta::default());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn failed_sweep_keeps_the_last_good_baseline() {
    let dir = tmp("poll-failure");
    let path = dir.join("a.png");
    std::fs::write(&path, b"old").unwrap();
    let mut root = PollingRoot::new(dir.clone(), String::from("injected watch failure"));
    let _ = complete(&mut root, 32);
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(matches!(root.advance(32), PollAdvance::Failed(_)));
    std::fs::create_dir_all(&dir).unwrap();
    let recovered = complete(&mut root, 32);
    assert_eq!(recovered.removed, vec![path]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn transient_and_internal_files_are_excluded() {
    let dir = tmp("poll-exclusions");
    std::fs::write(dir.join("video.mp4.part"), b"partial").unwrap();
    let internal = dir.join(".skwd-wall-v2/trash/images");
    std::fs::create_dir_all(&internal).unwrap();
    std::fs::write(internal.join("old.png"), b"old").unwrap();
    let final_path = dir.join("video.mp4");
    std::fs::write(&final_path, b"done").unwrap();
    let mut root = PollingRoot::new(dir.clone(), String::from("injected watch failure"));
    assert_eq!(complete(&mut root, 32).changed, vec![final_path]);
    let _ = std::fs::remove_dir_all(dir);
}
