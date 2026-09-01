use std::process::Command;

fn assert_version(flag: &str) {
    let output = Command::new(env!("CARGO_BIN_EXE_skwd-wall-scan"))
        .arg(flag)
        .output()
        .expect("run skwd-wall-scan version command");

    assert!(output.status.success(), "{flag}: {output:?}");
    assert_eq!(
        std::str::from_utf8(&output.stdout).expect("version output is UTF-8"),
        format!("skwd-wall-scan {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert!(output.stderr.is_empty(), "{flag}: {output:?}");
}

#[test]
fn long_version_flag() {
    assert_version("--version");
}

#[test]
fn short_version_flag() {
    assert_version("-V");
}
