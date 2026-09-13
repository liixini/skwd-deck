use super::*;

#[test]
fn capability_hidden_environment_uses_owning_terminal_config_and_bounds_cycles() {
    let root = tempfile::tempdir().unwrap();
    let child = root.path().join("2");
    let parent = root.path().join("1");
    std::fs::create_dir_all(&child).unwrap();
    std::fs::create_dir_all(&parent).unwrap();
    std::fs::write(child.join("status"), "PPid:\t1\n").unwrap();
    std::fs::write(parent.join("environ"), b"HOME=/user\0XDG_CONFIG_HOME=/private/config\0")
        .unwrap();
    assert_eq!(process_config(&child), Some(PathBuf::from("/private/config")));
    std::fs::write(child.join("environ"), b"HOME=/user\0XDG_CONFIG_HOME=/other/config\0").unwrap();
    assert_eq!(process_config(&child), Some(PathBuf::from("/other/config")));
    std::fs::remove_file(child.join("environ")).unwrap();
    std::fs::remove_file(parent.join("environ")).unwrap();
    std::fs::write(parent.join("status"), "PPid:\t2\n").unwrap();
    assert_eq!(process_config(&child), None);
}
