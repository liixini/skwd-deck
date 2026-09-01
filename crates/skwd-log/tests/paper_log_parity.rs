use std::path::PathBuf;

#[test]
fn files_matches_paper_log() {
    let ours = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/files.rs");
    let paper = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../skwd-paper/crates/paper-log/src/files.rs");
    let Ok(theirs) = std::fs::read_to_string(&paper) else {
        eprintln!("paper-log sibling checkout absent; parity check skipped");
        return;
    };
    let ours = std::fs::read_to_string(&ours).expect("read skwd-log files.rs");
    assert_eq!(ours, theirs, "drifted from paper-log");
}
