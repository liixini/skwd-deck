use super::*;

fn words(list: &[&str]) -> Vec<OsString> {
    list.iter().map(OsString::from).collect()
}

#[test]
fn parses_preparation() {
    let parsed = parse(words(&[
        "--pack",
        "p.json",
        "--input",
        "a.png",
        "--output",
        "out",
        "--source-sha256",
        "ab",
    ]))
    .unwrap();
    assert_eq!(parsed.pack, Some(PathBuf::from("p.json")));
    assert_eq!(parsed.output, Some(PathBuf::from("out")));
    assert_eq!(parsed.source_hash.as_deref(), Some("ab"));
    assert!(!parsed.check);
}

#[test]
fn parses_depth_map() {
    let parsed =
        parse(words(&["--pack", "p.json", "--input", "a.png", "--depth-map", "d.png"])).unwrap();
    assert_eq!(parsed.depth_map, Some(PathBuf::from("d.png")));
    assert_eq!(parsed.input, Some(PathBuf::from("a.png")));
}

#[test]
fn parses_check() {
    assert!(parse(words(&["--check", "--pack", "p.json"])).unwrap().check);
}

#[test]
fn rejects_bad_words() {
    assert!(parse(words(&["--verbose"])).is_err());
    assert!(parse(words(&["--pack"])).is_err());
    assert!(parse(words(&["--device", "rocm"])).is_err());
}

#[test]
fn parses_device() {
    assert_eq!(parse(words(&["--device", "cpu"])).unwrap().device, Some(Device::Cpu));
    assert_eq!(parse(words(&["--check"])).unwrap().device, None);
}
