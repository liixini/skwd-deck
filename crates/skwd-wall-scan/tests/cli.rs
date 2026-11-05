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

const FIXTURE: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../skwd-walld/assets/outdoors/ambient_day_nps.jpg");

fn scan(flag: &str, image: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_skwd-wall-scan"))
        .args([flag, image])
        .output()
        .expect("run skwd-wall-scan")
}

fn is_hex(value: &serde_json::Value) -> bool {
    value.as_str().is_some_and(|text| text.starts_with('#') && text.len() == 7)
}

#[test]
fn semantic_shape() {
    let output = scan("--semantic", FIXTURE);
    assert!(output.status.success(), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("semantic json");
    for key in ["bg", "surface", "fg", "dim", "accent"] {
        assert!(is_hex(&json[key]), "{key}: {json}");
    }
    assert!(json["dark"].is_boolean(), "{json}");
}

#[test]
fn theme_preview_shape() {
    let output = scan("--theme-preview", FIXTURE);
    assert!(output.status.success(), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("preview json");
    let colors = json.as_array().expect("preview array");
    assert!(!colors.is_empty());
    assert!(colors.iter().all(is_hex), "{json}");
}

#[test]
fn semantic_undecodable_is_empty() {
    let path =
        std::env::temp_dir().join(format!("skwd-scan-undecodable-{}.png", std::process::id()));
    std::fs::write(&path, b"not an image").expect("write fixture");
    let output = scan("--semantic", path.to_str().expect("utf-8 path"));
    let _ = std::fs::remove_file(&path);
    assert!(output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
}
