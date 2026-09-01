use std::process::Command;

fn helm() -> Command {
    Command::new(env!("CARGO_BIN_EXE_skwd-helm"))
}

#[test]
fn version_output() {
    let output = helm().arg("--version").output().unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("skwd-helm {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn no_args_and_help() {
    for arguments in [&[][..], &["--help"][..]] {
        let output = helm().args(arguments).output().unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.starts_with("skwd-helm - control client"));
        assert!(!stdout.contains("launch the picker"));
    }
}

#[test]
fn unknown_verb_exit_code() {
    let output = helm().arg("definitely-not-a-verb").output().unwrap();
    assert_eq!(output.status.code(), Some(4));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown verb"));
}
